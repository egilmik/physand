//! Top-level game state: the run, the current floor, the player, enemies,
//! spell particles and camera.

use physand_sim::{
    id,
    material::props,
    rng::Rng,
    worldgen::{dune_catacombs, Level, Rect},
    GpuCell, World,
};

use crate::enemy::{Enemy, EnemyEvent, EnemyKind, BAKE_TEMP};
use crate::input::Input;
use crate::player::{Player, PLAYER_H, PLAYER_W};
use crate::render::{self, Overlay, Rgba, Tint};
use crate::spell::{starter_flasks, CastCtx, Catalyst, Flask, Flight, Particle, ParticleKind};

pub const WORLD_W: u32 = 1024;
pub const WORLD_H: u32 = 768;
pub const VIEW_W: u32 = 480;
pub const VIEW_H: u32 = 270;

/// Dig reach from the player's centre, in cells.
pub const DIG_REACH: f32 = 22.0;
pub const DIG_RADIUS: i32 = 4;
const CAMERA_LERP: f32 = 0.12;
/// How far the camera leans toward the aim point (0 = locked on player).
const CAMERA_LOOKAHEAD: f32 = 0.2;
/// Most damage the player can take from explosions in a single tick, so a
/// chain reaction of tiny blasts is not instantly lethal.
const MAX_BLAST_DAMAGE_PER_TICK: f32 = 25.0;
/// HP restored when descending to a new floor.
const DESCEND_HEAL: f32 = 30.0;

/// Where the run stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Playing,
    /// The alchemist died; the run is over.
    Dead,
    /// Reached the floor's exit.
    Cleared,
}

impl Phase {
    pub fn code(self) -> u8 {
        match self {
            Phase::Playing => 0,
            Phase::Dead => 1,
            Phase::Cleared => 2,
        }
    }
}

pub struct Game {
    pub world: World,
    pub player: Player,
    pub flasks: Vec<Flask>,
    pub enemies: Vec<Enemy>,
    active_flask: usize,
    particles: Vec<Particle>,
    exit: Option<Rect>,
    seal: Option<Rect>,
    seal_open: bool,
    input: Input,
    prev_jump: bool,
    rng: Rng,
    cam_x: f32,
    cam_y: f32,
    view_cells: Vec<GpuCell>,
    overlay: Vec<Rgba>,
    pub phase: Phase,
    run_seed: u32,
    /// Floor number, starting at 1.
    pub depth: u32,
    pub kills: u32,
    /// Ticks spent playing this run / this floor.
    pub run_ticks: u32,
    pub floor_ticks: u32,
    /// Deaths this run (0 or 1 with permadeath; kept for stats).
    pub deaths: u32,
}

impl Game {
    /// Start a new run on floor 1.
    pub fn new(seed: u32) -> Game {
        Self::from_level(dune_catacombs(WORLD_W, WORLD_H, seed), seed)
    }

    /// Start on a prepared level (tests and sandboxes). Enemies are placed at
    /// the level's spawn spots, if it has any.
    pub fn from_level(level: Level, seed: u32) -> Game {
        let mut game = Game {
            world: World::new(1, 1, seed),
            player: Player::new(level.spawn.0, level.spawn.1),
            flasks: starter_flasks(),
            enemies: Vec::new(),
            active_flask: 0,
            particles: Vec::new(),
            exit: None,
            seal: None,
            seal_open: false,
            input: Input::default(),
            prev_jump: false,
            rng: Rng::new(seed ^ 0x5EED_0001),
            cam_x: 0.0,
            cam_y: 0.0,
            view_cells: vec![[0; 4]; (VIEW_W * VIEW_H) as usize],
            overlay: vec![[0; 4]; (VIEW_W * VIEW_H) as usize],
            phase: Phase::Playing,
            run_seed: seed,
            depth: 1,
            kills: 0,
            run_ticks: 0,
            floor_ticks: 0,
            deaths: 0,
        };
        game.enter_floor(level);
        game
    }

    /// Install a floor: world, markers, enemies, player at the spawn.
    fn enter_floor(&mut self, level: Level) {
        let Level {
            world,
            spawn,
            exit,
            seal,
            ground_spots,
            air_spots,
            room_spots,
        } = level;
        self.world = world;
        self.exit = exit;
        self.seal = seal;
        self.seal_open = seal.is_none();
        self.particles.clear();
        self.floor_ticks = 0;
        self.phase = Phase::Playing;

        let hp = self.player.hp;
        let stamina = self.player.stamina;
        self.player = Player::new(spawn.0, spawn.1);
        self.player.hp = hp;
        self.player.stamina = stamina;

        self.enemies = self.populate(&ground_spots, &air_spots, &room_spots);

        let (tx, ty) = self.camera_target();
        self.cam_x = tx;
        self.cam_y = ty;
    }

    /// Choose enemies for the spawn spots; deeper floors are busier.
    fn populate(
        &mut self,
        ground: &[(i32, i32)],
        air: &[(i32, i32)],
        rooms: &[(i32, i32)],
    ) -> Vec<Enemy> {
        let d = self.depth as usize;
        let mut enemies = Vec::new();
        let golems = (1 + d).min(rooms.len());
        for &(x, y) in rooms.iter().take(golems) {
            enemies.push(Enemy::new(EnemyKind::Golem, x, y));
        }
        let scarabs = 10 + 3 * d;
        for &(x, y) in ground
            .iter()
            .filter(|s| !rooms[..golems].contains(s))
            .take(scarabs)
        {
            enemies.push(Enemy::new(EnemyKind::Scarab, x, y));
        }
        for &(x, y) in air.iter().take(5 + 2 * d) {
            enemies.push(Enemy::new(EnemyKind::Wraith, x, y));
        }
        enemies
    }

    /// Generate and enter the next floor (only after clearing this one).
    pub fn descend(&mut self) -> bool {
        if self.phase != Phase::Cleared {
            return false;
        }
        self.depth += 1;
        let seed = self
            .run_seed
            .wrapping_mul(0x9E37_79B9)
            .wrapping_add(self.depth.wrapping_mul(0x85EB_CA6B));
        self.player.heal(DESCEND_HEAL);
        for f in &mut self.flasks {
            f.refill();
        }
        self.enter_floor(dune_catacombs(WORLD_W, WORLD_H, seed));
        true
    }

    /// Spawn an enemy (tests, debugging).
    pub fn add_enemy(&mut self, kind: EnemyKind, x: i32, y: i32) {
        self.enemies.push(Enemy::new(kind, x, y));
    }

    /// Debug: move the player into the antechamber just above the seal.
    pub fn debug_warp_to_seal(&mut self) {
        if let Some(s) = self.seal {
            let (cx, _) = s.center();
            self.player.body = crate::body::Body::standing(cx - 10, s.y - 1, PLAYER_W, PLAYER_H);
        }
    }

    pub fn set_input(&mut self, input: Input) {
        if input.jump && !self.prev_jump {
            self.player.press_jump();
        }
        self.prev_jump = input.jump;
        self.input = input;
    }

    /// Camera position (top-left of the view, world cells), already rounded.
    pub fn camera(&self) -> (i32, i32) {
        (self.cam_x.round() as i32, self.cam_y.round() as i32)
    }

    /// Convert a view-space position to world space.
    pub fn view_to_world(&self, vx: f32, vy: f32) -> (f32, f32) {
        let (cx, cy) = self.camera();
        (vx + cx as f32, vy + cy as f32)
    }

    pub fn particle_count(&self) -> usize {
        self.particles.len()
    }

    pub fn active_flask(&self) -> usize {
        self.active_flask
    }

    pub fn flask(&self) -> &Flask {
        &self.flasks[self.active_flask]
    }

    pub fn select_flask(&mut self, index: usize) {
        if index < self.flasks.len() {
            self.active_flask = index;
        }
    }

    /// Cycle the active flask by `delta` slots (mouse wheel).
    pub fn cycle_flask(&mut self, delta: i32) {
        let n = self.flasks.len() as i32;
        self.active_flask = (self.active_flask as i32 + delta).rem_euclid(n) as usize;
    }

    pub fn exit(&self) -> Option<Rect> {
        self.exit
    }

    pub fn seal(&self) -> Option<Rect> {
        self.seal
    }

    /// Whether the floor's wax seal has been melted through.
    pub fn seal_open(&self) -> bool {
        self.seal_open
    }

    pub fn enemies_alive(&self) -> usize {
        self.enemies.len()
    }

    pub fn step(&mut self) {
        let playing = self.phase == Phase::Playing && self.player.alive();
        let input = if playing {
            self.input
        } else {
            Input {
                aim_x: self.input.aim_x,
                aim_y: self.input.aim_y,
                ..Input::default()
            }
        };

        if playing {
            if input.dig {
                self.dig(input.aim_x, input.aim_y);
            }
            if input.cast {
                // Turn to face the spell so it leaves from the near hand.
                let (px, _) = self.player.center();
                if (input.aim_x - px).abs() > 1.0 {
                    self.player.facing = if input.aim_x > px { 1 } else { -1 };
                }
                let origin = self.hand();
                let mut ctx = CastCtx {
                    world: &mut self.world,
                    rng: &mut self.rng,
                    particles: &mut self.particles,
                    origin,
                    target: (input.aim_x, input.aim_y),
                };
                self.flasks[self.active_flask].cast(&mut ctx);
            }
            self.player.update(&mut self.world, &input);
        }

        self.update_enemies(playing);
        for flask in &mut self.flasks {
            flask.tick();
        }
        self.update_particles();
        self.world.step();
        self.apply_explosions();
        self.reap_enemies();
        self.check_progress();
        self.update_camera();
    }

    fn update_enemies(&mut self, playing: bool) {
        let target = playing.then(|| self.player.body.clone());
        for enemy in &mut self.enemies {
            if let Some(EnemyEvent::Hit { damage, knock_x }) =
                enemy.update(&mut self.world, target.as_ref(), &mut self.rng)
            {
                self.player.hit(damage, knock_x);
            }
        }
    }

    fn update_particles(&mut self) {
        let enemies = &self.enemies;
        let hit_test = |x: f32, y: f32| {
            enemies
                .iter()
                .position(|e| e.alive() && e.body.contains(x, y))
        };
        let (world, rng) = (&mut self.world, &mut self.rng);
        let mut struck = Vec::new();
        self.particles
            .retain_mut(|p| match p.update(world, rng, hit_test) {
                Flight::Flying => true,
                Flight::Landed => false,
                Flight::HitCreature(i) => {
                    struck.push((i, p.clone()));
                    false
                }
            });
        for (i, p) in struck {
            self.enemies[i].struck_by(&p);
        }
    }

    /// Blasts hurt and knock back the player and enemies.
    fn apply_explosions(&mut self) {
        let blasts = self.world.take_explosions();
        if blasts.is_empty() {
            return;
        }
        let mut player_damage = 0.0;
        for b in &blasts {
            let reach = b.radius as f32 * 2.5 + 4.0;
            let strength = (b.radius as f32 / 7.0).clamp(0.4, 1.5);
            let push = |center: (f32, f32)| {
                let (dx, dy) = (center.0 - b.x as f32, center.1 - b.y as f32);
                let dist = (dx * dx + dy * dy).sqrt();
                (dist < reach).then(|| {
                    let falloff = 1.0 - dist / reach;
                    let (nx, ny) = if dist > 0.01 {
                        (dx / dist, dy / dist)
                    } else {
                        (0.0, -1.0)
                    };
                    (falloff, nx, ny)
                })
            };
            if self.player.alive() {
                if let Some((f, nx, ny)) = push(self.player.center()) {
                    player_damage += 30.0 * f * strength;
                    self.player.vx += nx * 4.0 * f;
                    self.player.vy += ny * 4.0 * f - 0.5 * f;
                }
            }
            for e in &mut self.enemies {
                if let Some((f, nx, ny)) = push(e.body.center()) {
                    e.credited = true;
                    e.damage(40.0 * f * strength);
                    e.body.vx += nx * 4.0 * f;
                    e.body.vy += ny * 4.0 * f - 0.5 * f;
                }
            }
        }
        if player_damage > 0.0 && self.phase == Phase::Playing {
            self.player
                .damage(player_damage.min(MAX_BLAST_DAMAGE_PER_TICK));
        }
    }

    /// Remove dead enemies, leaving their remains in the world.
    fn reap_enemies(&mut self) {
        let world = &mut self.world;
        let mut killed = 0;
        self.enemies.retain(|e| {
            if e.alive() {
                return true;
            }
            e.remains(world);
            killed += e.credited as u32;
            false
        });
        if self.phase == Phase::Playing {
            self.kills += killed;
        }
    }

    fn check_progress(&mut self) {
        if self.phase != Phase::Playing {
            return;
        }
        self.run_ticks += 1;
        self.floor_ticks += 1;
        if !self.player.alive() {
            self.phase = Phase::Dead;
            self.deaths += 1;
            return;
        }
        if !self.seal_open && self.floor_ticks.is_multiple_of(15) {
            if let Some(s) = self.seal {
                // Open once a gap wide enough for the alchemist has been
                // melted all the way through.
                let gap = PLAYER_W + 1;
                self.seal_open = (s.x..=s.x + s.w - gap).any(|x0| {
                    (x0..x0 + gap)
                        .all(|x| (s.y..s.y + s.h).all(|y| self.world.get(x, y) != id::WAX))
                });
            }
        }
        if let Some(exit) = self.exit {
            // The rift devours any material that falls into it, so sand
            // pouring through the open seal can never bury the way out.
            for y in exit.y..exit.y + exit.h {
                for x in exit.x..exit.x + exit.w {
                    if self.world.get(x, y) != id::EMPTY {
                        self.world.set(x, y, id::EMPTY);
                    }
                }
            }
            let (px, py) = self.player.center();
            if exit.contains(px as i32, py as i32) {
                self.phase = Phase::Cleared;
            }
        }
    }

    /// Where spells leave the player: the flask hand.
    fn hand(&self) -> (f32, f32) {
        let (cx, _) = self.player.center();
        (
            cx + self.player.facing as f32 * 2.0,
            self.player.y as f32 + 5.5,
        )
    }

    /// Aim point clamped to dig reach.
    pub fn dig_target(&self, aim_x: f32, aim_y: f32) -> (i32, i32) {
        let (px, py) = self.player.center();
        let (dx, dy) = (aim_x - px, aim_y - py);
        let dist = (dx * dx + dy * dy).sqrt();
        let k = if dist > DIG_REACH {
            DIG_REACH / dist
        } else {
            1.0
        };
        ((px + dx * k).floor() as i32, (py + dy * k).floor() as i32)
    }

    fn dig(&mut self, aim_x: f32, aim_y: f32) {
        let (tx, ty) = self.dig_target(aim_x, aim_y);
        let r = DIG_RADIUS;
        for y in (ty - r)..=(ty + r) {
            for x in (tx - r)..=(tx + r) {
                if (x - tx).pow(2) + (y - ty).pow(2) > r * r {
                    continue;
                }
                let chance = props(self.world.get(x, y)).dig;
                if chance > 0.0 && self.rng.chance(chance) && self.world.cell(x, y).is_some() {
                    self.world.set(x, y, id::EMPTY);
                }
            }
        }
    }

    fn camera_target(&self) -> (f32, f32) {
        let (px, py) = self.player.center();
        let lead_x = (self.input.aim_x - px) * CAMERA_LOOKAHEAD;
        let lead_y = (self.input.aim_y - py) * CAMERA_LOOKAHEAD;
        let lead = |v: f32| {
            if v.is_finite() {
                v.clamp(-60.0, 60.0)
            } else {
                0.0
            }
        };
        let max_x = (self.world.width() as f32 - VIEW_W as f32).max(0.0);
        let max_y = (self.world.height() as f32 - VIEW_H as f32).max(0.0);
        (
            (px + lead(lead_x) - VIEW_W as f32 / 2.0).clamp(0.0, max_x),
            (py + lead(lead_y) - VIEW_H as f32 / 2.0).clamp(0.0, max_y),
        )
    }

    fn update_camera(&mut self) {
        let (tx, ty) = self.camera_target();
        self.cam_x += (tx - self.cam_x) * CAMERA_LERP;
        self.cam_y += (ty - self.cam_y) * CAMERA_LERP;
    }

    /// Where the compass points: the seal until it is open, then the exit.
    pub fn objective(&self) -> Option<(f32, f32)> {
        let r = if self.seal_open {
            self.exit?
        } else {
            self.seal?
        };
        let (x, y) = r.center();
        Some((x as f32, y as f32))
    }

    /// Compose the visible window: cells into `view_cells`, sprites into `overlay`.
    pub fn render(&mut self) {
        let (cx, cy) = self.camera();
        let (vw, vh) = (VIEW_W as i32, VIEW_H as i32);
        self.world.copy_region(cx, cy, vw, vh, &mut self.view_cells);

        let flask_color = {
            let f = self.flask();
            let [r, g, b] = f.color();
            let glow = if f.volume >= f.cost() { 255 } else { 120 };
            [r, g, b, glow]
        };
        let playing = self.phase == Phase::Playing && self.player.alive();
        let dig_ring = (playing && self.input.dig)
            .then(|| self.dig_target(self.input.aim_x, self.input.aim_y));
        let objective = self.objective();
        let tick = self.world.tick_count();
        let mut o = Overlay {
            buf: &mut self.overlay,
            w: vw,
            h: vh,
        };
        o.clear();

        if let Some(exit) = self.exit {
            let (ex, ey) = exit.center();
            o.portal(ex - cx, ey - cy, exit.w / 2 + 2, exit.h / 2, tick);
        }

        for e in &self.enemies {
            let b = &e.body;
            let tint = if e.hurt_flash > 0 {
                Some(Tint::Hurt)
            } else if e.frozen > 0 {
                Some(Tint::Frozen)
            } else if e.burning > 0 && (tick / 3).is_multiple_of(2) {
                Some(Tint::Burning)
            } else if e.kind == EnemyKind::Golem && e.temp > 80.0 {
                Some(Tint::Hot(
                    ((e.temp - 80.0) / (BAKE_TEMP - 80.0) * 255.0).min(255.0) as u8,
                ))
            } else {
                None
            };
            let frame = (e.anim / 8) % 2 == 0;
            let (x, y) = (b.x - cx, b.y - cy);
            match e.kind {
                EnemyKind::Scarab => {
                    let rows: &[&str] = if frame {
                        &render::SCARAB
                    } else {
                        &render::SCARAB_STEP
                    };
                    o.sprite(x, y, rows, render::creature_palette, e.facing, tint);
                }
                EnemyKind::Wraith => {
                    let rows: &[&str] = if frame {
                        &render::WRAITH_UP
                    } else {
                        &render::WRAITH_DOWN
                    };
                    o.sprite(x, y, rows, render::creature_palette, e.facing, tint);
                }
                EnemyKind::Golem => {
                    o.sprite(x, y, &render::GOLEM, render::golem_palette, e.facing, tint)
                }
            }
        }

        for p in &self.particles {
            let mut c = props(p.mat()).color.map(|v| v.saturating_add(40));
            // Tint by temperature so hot and cold spells read at a glance.
            if p.payload.impact_heat > 0 {
                c = [255, c[1].max(150), c[2] / 2];
            } else if p.payload.impact_heat < 0 {
                c = [c[0] / 2 + 100, c[1] / 2 + 120, 255];
            }
            let (x, y) = (p.x.floor() as i32 - cx, p.y.floor() as i32 - cy);
            match p.kind {
                ParticleKind::Droplet => o.put(x, y, [c[0], c[1], c[2], 255]),
                ParticleKind::Orb => {
                    for (dx, dy) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
                        o.put(x + dx, y + dy, [c[0], c[1], c[2], 255]);
                    }
                    let rim = if p.payload.volatile {
                        [255, 60, 40, 220]
                    } else {
                        [255, 255, 255, 120]
                    };
                    for (dx, dy) in [(1, 1), (-1, 1), (1, -1), (-1, -1)] {
                        o.put(x + dx, y + dy, rim);
                    }
                }
            }
        }

        let pl = &self.player;
        if pl.alive() {
            let blink = pl.invuln > 0 && (tick / 4).is_multiple_of(2);
            if !blink {
                o.player(
                    pl.x - cx,
                    pl.y - cy,
                    pl.facing,
                    pl.hurt_flash > 0,
                    flask_color,
                );
            }
        }
        if playing {
            if let Some((tx, ty)) = dig_ring {
                o.ring(tx - cx, ty - cy, DIG_RADIUS, [255, 220, 150, 140]);
            }
            if let Some((tx, ty)) = objective {
                o.compass(tx - cx as f32, ty - cy as f32, [255, 205, 90, 230]);
            }
            let (ax, ay) = (
                self.input.aim_x.floor() as i32,
                self.input.aim_y.floor() as i32,
            );
            o.reticle(ax - cx, ay - cy, [255, 255, 255, 180]);
        }
    }

    /// RGBA8 cell data for the view, `VIEW_W * VIEW_H * 4` bytes.
    pub fn view_cells(&self) -> &[GpuCell] {
        &self.view_cells
    }

    pub fn overlay(&self) -> &[Rgba] {
        &self.overlay
    }

    /// Paint in view coordinates (sandbox brush).
    pub fn paint_view(&mut self, vx: i32, vy: i32, radius: i32, mat: u8) {
        let (cx, cy) = self.camera();
        self.world.paint(vx + cx, vy + cy, radius, mat);
    }

    pub fn player_size() -> (i32, i32) {
        (PLAYER_W, PLAYER_H)
    }

    /// Reconfigure a flask from wasm-friendly codes (`255` = empty slot).
    /// Returns `false` if the recipe is invalid.
    pub fn configure_flask(
        &mut self,
        index: usize,
        reagents: &[u8],
        form: u8,
        catalysts: &[u8],
    ) -> bool {
        use crate::spell::Form;
        let Some(flask) = self.flasks.get_mut(index) else {
            return false;
        };
        let reagents: Vec<u8> = reagents.iter().copied().filter(|&r| r != 255).collect();
        let Some(&form) = Form::ALL.get(form as usize) else {
            return false;
        };
        let catalysts: Option<Vec<Catalyst>> = catalysts
            .iter()
            .filter(|&&c| c != 255)
            .map(|&c| Catalyst::ALL.get(c as usize).copied())
            .collect();
        match catalysts {
            Some(cats) => flask.configure(&reagents, form, &cats).is_ok(),
            None => false,
        }
    }
}
