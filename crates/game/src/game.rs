//! Top-level game state: the run (stage, inventory, gold), the current level,
//! the player, creatures, spell particles and camera.

use physand_sim::{
    id,
    material::props,
    rng::Rng,
    worldgen::{distillery, dune_catacombs, wyrm_arena, Level, Marker, Rect},
    GpuCell, World,
};

use crate::distillery::{Distillery, Interaction, Prompt, StationKind};
use crate::enemy::{Enemy, EnemyEvent, EnemyKind, BAKE_TEMP};
use crate::input::Input;
use crate::player::{Player, PLAYER_H, PLAYER_W};
use crate::render::{self, Overlay, Rgba, Tint};
use crate::run::{Inventory, Item, Modifiers, CAPACITY_BONUS, FLOORS_PER_RUN, HEAL_AMOUNT};
use crate::spell::{
    starter_flasks, CastCtx, Catalyst, Flask, Flight, Form, Particle, ParticleKind, BASE_REGEN,
};
use crate::wyrm::{Wyrm, HEAD_RADIUS, SEGMENT_RADIUS};

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
/// The Distillery fountain restores this fraction of maximum health.
const FOUNTAIN_HEAL: f32 = 0.25;

/// Where the run stands right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Playing,
    /// The alchemist died; the run is over.
    Dead,
    /// Reached a floor's exit.
    Cleared,
    /// The Sandwyrm is slain and its rift taken: the run is won.
    Victory,
}

impl Phase {
    pub fn code(self) -> u8 {
        match self {
            Phase::Playing => 0,
            Phase::Dead => 1,
            Phase::Cleared => 2,
            Phase::Victory => 3,
        }
    }
}

/// Which part of the run the current level is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// A catacomb floor (1-based).
    Floor(u32),
    /// The safe room after the given floor.
    Distillery(u32),
    /// The Sandwyrm's lair.
    Boss,
}

impl Stage {
    pub fn code(self) -> u8 {
        match self {
            Stage::Floor(_) => 0,
            Stage::Distillery(_) => 1,
            Stage::Boss => 2,
        }
    }
}

/// A creature a particle can strike.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Enemy(usize),
    Segment(usize),
}

pub struct Game {
    pub world: World,
    pub player: Player,
    pub flasks: Vec<Flask>,
    pub enemies: Vec<Enemy>,
    pub wyrm: Option<Wyrm>,
    pub inventory: Inventory,
    pub gold: u32,
    pub stage: Stage,
    /// Sandbox games (tests, prepared levels) own everything and may edit
    /// flasks anywhere.
    pub sandbox: bool,
    distillery: Option<Distillery>,
    active_flask: usize,
    particles: Vec<Particle>,
    exit: Option<Rect>,
    seal: Option<Rect>,
    seal_open: bool,
    rift_spot: Option<Rect>,
    input: Input,
    prev_jump: bool,
    prev_interact: bool,
    pending_interact: bool,
    pub last_interaction: Option<Interaction>,
    mods: Modifiers,
    rng: Rng,
    cam_x: f32,
    cam_y: f32,
    view_cells: Vec<GpuCell>,
    overlay: Vec<Rgba>,
    pub phase: Phase,
    run_seed: u32,
    /// Deepest floor reached, starting at 1.
    pub depth: u32,
    pub kills: u32,
    /// Ticks spent playing this run / this level.
    pub run_ticks: u32,
    pub floor_ticks: u32,
    /// Deaths this run (0 or 1 with permadeath; kept for stats).
    pub deaths: u32,
}

impl Game {
    /// Start a new run on floor 1 with the starter kit.
    pub fn new(seed: u32) -> Game {
        let mut g = Self::bare(seed, (0, 0));
        g.sandbox = false;
        g.inventory = Inventory::starter();
        g.enter_level(dune_catacombs(WORLD_W, WORLD_H, seed), Stage::Floor(1));
        g
    }

    /// A sandbox game on a prepared level (tests, experiments): everything
    /// is owned and flasks can be edited anywhere. Enemies are placed at the
    /// level's spawn spots, if it has any.
    pub fn from_level(level: Level, seed: u32) -> Game {
        let mut g = Self::bare(seed, level.spawn);
        g.enter_level(level, Stage::Floor(1));
        g
    }

    fn bare(seed: u32, spawn: (i32, i32)) -> Game {
        Game {
            world: World::new(1, 1, seed),
            player: Player::new(spawn.0, spawn.1),
            flasks: starter_flasks(),
            enemies: Vec::new(),
            wyrm: None,
            inventory: Inventory::everything(),
            gold: 0,
            stage: Stage::Floor(1),
            sandbox: true,
            distillery: None,
            active_flask: 0,
            particles: Vec::new(),
            exit: None,
            seal: None,
            seal_open: false,
            rift_spot: None,
            input: Input::default(),
            prev_jump: false,
            prev_interact: false,
            pending_interact: false,
            last_interaction: None,
            mods: Modifiers::default(),
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
        }
    }

    /// Install a level: world, markers, creatures, player at the spawn.
    fn enter_level(&mut self, level: Level, stage: Stage) {
        self.stage = stage;
        self.distillery = match stage {
            Stage::Distillery(_) => Some(Distillery::stock(&level, &self.inventory, &mut self.rng)),
            _ => None,
        };
        self.wyrm = match stage {
            Stage::Boss => level.marked(Marker::BossLair).first().map(|r| {
                let (x, y) = r.center();
                Wyrm::new(x as f32, y as f32)
            }),
            _ => None,
        };
        self.rift_spot = level.marked(Marker::RiftSpot).first().copied();

        let Level {
            world,
            spawn,
            exit,
            seal,
            ground_spots,
            air_spots,
            room_spots,
            markers: _,
        } = level;
        self.world = world;
        self.exit = exit;
        self.seal = seal;
        self.seal_open = seal.is_none();
        self.particles.clear();
        self.floor_ticks = 0;
        self.phase = Phase::Playing;
        self.last_interaction = None;

        // Keep health, stamina and relic effects across levels.
        let old = self.player.clone();
        self.player = Player::new(spawn.0, spawn.1);
        self.player.max_hp = old.max_hp;
        self.player.hp = old.hp;
        self.player.stamina = old.stamina;
        self.apply_modifiers();

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

    fn level_seed(&self, salt: u32) -> u32 {
        self.run_seed
            .wrapping_mul(0x9E37_79B9)
            .wrapping_add(salt.wrapping_mul(0x85EB_CA6B))
    }

    /// After clearing a floor: enter the Distillery. Returns `false` unless
    /// a floor has just been cleared.
    pub fn descend(&mut self) -> bool {
        let Stage::Floor(n) = self.stage else {
            return false;
        };
        if self.phase != Phase::Cleared {
            return false;
        }
        let level = distillery(self.level_seed(1000 + n));
        self.enter_level(level, Stage::Distillery(n));
        true
    }

    /// Leave the Distillery through its rift: the next floor, or the boss.
    fn leave_distillery(&mut self) {
        let Stage::Distillery(n) = self.stage else {
            return;
        };
        if n < FLOORS_PER_RUN {
            self.depth = n + 1;
            let level = dune_catacombs(WORLD_W, WORLD_H, self.level_seed(n + 1));
            self.enter_level(level, Stage::Floor(n + 1));
        } else {
            self.enter_level(wyrm_arena(self.level_seed(99)), Stage::Boss);
        }
    }

    /// Recompute relic effects on the player and flasks.
    fn apply_modifiers(&mut self) {
        self.mods = self.inventory.modifiers();
        self.player.apply(&self.mods);
        for f in &mut self.flasks {
            f.regen = BASE_REGEN * self.mods.flask_regen;
        }
    }

    /// Give the player an item.
    pub fn grant(&mut self, item: Item) {
        match item {
            Item::Heal => self.player.heal(HEAL_AMOUNT),
            Item::Capacity => {
                for f in &mut self.flasks {
                    f.capacity += CAPACITY_BONUS;
                    f.refill();
                }
            }
            _ => {
                self.inventory.add(item);
                self.apply_modifiers();
            }
        }
    }

    pub fn modifiers(&self) -> &Modifiers {
        &self.mods
    }

    // ---------------------------------------------------------------------
    // Debug and test helpers
    // ---------------------------------------------------------------------

    /// Spawn an enemy (tests, debugging).
    pub fn add_enemy(&mut self, kind: EnemyKind, x: i32, y: i32) {
        self.enemies.push(Enemy::new(kind, x, y));
    }

    /// Spawn the Sandwyrm (tests, debugging).
    pub fn spawn_wyrm(&mut self, x: f32, y: f32) {
        self.wyrm = Some(Wyrm::new(x, y));
    }

    /// Debug: move the player into the antechamber just above the seal.
    pub fn debug_warp_to_seal(&mut self) {
        if let Some(s) = self.seal {
            let (cx, _) = s.center();
            self.player.body = crate::body::Body::standing(cx - 10, s.y - 1, PLAYER_W, PLAYER_H);
        }
    }

    /// Debug: count the current floor as cleared.
    pub fn debug_clear_floor(&mut self) {
        if matches!(self.stage, Stage::Floor(_)) && self.phase == Phase::Playing {
            self.phase = Phase::Cleared;
        }
    }

    /// Debug: jump straight to the boss.
    pub fn debug_skip_to_boss(&mut self) {
        self.depth = FLOORS_PER_RUN;
        self.stage = Stage::Distillery(FLOORS_PER_RUN);
        self.leave_distillery();
    }

    /// Debug: step into the current exit, if there is one.
    pub fn debug_enter_exit(&mut self) {
        if let Some(e) = self.exit {
            let (ex, ey) = e.center();
            self.player.body = crate::body::Body::standing(ex, ey + 4, PLAYER_W, PLAYER_H);
        }
    }

    /// Debug: slay the boss.
    pub fn debug_slay_boss(&mut self) {
        if let Some(w) = &mut self.wyrm {
            w.segments[0].hp = 0.0;
        }
    }

    // ---------------------------------------------------------------------
    // Accessors
    // ---------------------------------------------------------------------

    pub fn set_input(&mut self, input: Input) {
        if input.jump && !self.prev_jump {
            self.player.press_jump();
        }
        if input.interact && !self.prev_interact {
            self.pending_interact = true;
        }
        self.prev_jump = input.jump;
        self.prev_interact = input.interact;
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

    pub fn distillery(&self) -> Option<&Distillery> {
        self.distillery.as_ref()
    }

    /// Index of the Distillery station the player is standing at.
    fn station_here(&self) -> Option<usize> {
        let (px, py) = self.player.center();
        self.distillery.as_ref()?.at(px, py)
    }

    /// The prompt for the station the player is standing at.
    pub fn interact_prompt(&self) -> Option<Prompt> {
        let i = self.station_here()?;
        Some(self.distillery.as_ref()?.prompt(i, self.gold))
    }

    /// Flasks can be edited in sandboxes, or at a Distillery workbench.
    pub fn can_edit_flasks(&self) -> bool {
        if self.sandbox {
            return true;
        }
        match (self.station_here(), &self.distillery) {
            (Some(i), Some(d)) => d.stations[i].kind == StationKind::Workbench,
            _ => false,
        }
    }

    /// Boss health (current, maximum), if a boss is present.
    pub fn boss_health(&self) -> Option<(f32, f32)> {
        self.wyrm.as_ref().map(|w| (w.total_hp(), w.max_hp()))
    }

    // ---------------------------------------------------------------------
    // Simulation
    // ---------------------------------------------------------------------

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
            if std::mem::take(&mut self.pending_interact) {
                self.interact();
            }
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
            // Collect before moving too, or loose gold inside the body would
            // be shoved aside like sand instead of picked up.
            self.collect_gold();
            self.player.update(&mut self.world, &input);
            self.collect_gold();
        } else {
            self.pending_interact = false;
        }

        self.update_creatures(playing);
        for flask in &mut self.flasks {
            flask.tick();
        }
        self.update_particles();
        self.world.step();
        self.apply_explosions();
        self.reap_creatures();
        self.check_progress();
        self.update_camera();
    }

    /// Use the Distillery station the player is standing at.
    fn interact(&mut self) {
        let Some(i) = self.station_here() else { return };
        let Some(d) = self.distillery.as_mut() else {
            return;
        };
        let station = d.stations[i];
        let result = match station.kind {
            StationKind::Reward(item) => {
                d.take_reward();
                self.grant(item);
                Interaction::Took(item)
            }
            StationKind::Shop(item) => {
                let price = item.price();
                if self.gold < price {
                    Interaction::TooPoor { price }
                } else {
                    self.gold -= price;
                    d.stations[i].spent = true;
                    self.grant(item);
                    Interaction::Bought(item)
                }
            }
            StationKind::Fountain if station.spent => Interaction::AlreadyUsed,
            StationKind::Fountain => {
                d.stations[i].spent = true;
                for f in &mut self.flasks {
                    f.refill();
                }
                let heal = self.player.max_hp * FOUNTAIN_HEAL;
                self.player.heal(heal);
                Interaction::Refilled
            }
            StationKind::Workbench => Interaction::OpenSpellcraft,
        };
        self.last_interaction = Some(result);
    }

    /// Pick up gold the alchemist touches (or that the Magnet Ring pulls in).
    fn collect_gold(&mut self) {
        let reach = self.mods.magnet.max(1);
        let b = &self.player.body;
        let (x0, y0, x1, y1) = (
            b.x - reach,
            b.y - reach,
            b.x + b.w + reach,
            b.y + b.h + reach,
        );
        for y in y0..y1 {
            for x in x0..x1 {
                if self.world.get(x, y) == id::GOLD {
                    self.world.set(x, y, id::EMPTY);
                    self.gold += 1;
                }
            }
        }
    }

    fn update_creatures(&mut self, playing: bool) {
        let target = playing.then(|| self.player.body.clone());
        for enemy in &mut self.enemies {
            if let Some(EnemyEvent::Hit { damage, knock_x }) =
                enemy.update(&mut self.world, target.as_ref(), &mut self.rng)
            {
                self.player.hit(damage, knock_x);
            }
        }
        if let Some(w) = &mut self.wyrm {
            if let Some(EnemyEvent::Hit { damage, knock_x }) =
                w.update(&mut self.world, target.as_ref(), &mut self.rng)
            {
                self.player.hit(damage, knock_x);
            }
        }
    }

    /// The creature (if any) occupying a point.
    pub fn creature_at(&self, x: f32, y: f32) -> Option<Target> {
        if let Some(i) = self
            .enemies
            .iter()
            .position(|e| e.alive() && e.body.contains(x, y))
        {
            return Some(Target::Enemy(i));
        }
        self.wyrm.as_ref()?.segment_at(x, y).map(Target::Segment)
    }

    fn update_particles(&mut self) {
        // Encode targets as one index for the particle: enemies first, then
        // wyrm segments. Nothing is added or removed until after the loop.
        let n_enemies = self.enemies.len();
        let enemies = &self.enemies;
        let wyrm = self.wyrm.as_ref();
        let hit_test = |x: f32, y: f32| {
            if let Some(i) = enemies
                .iter()
                .position(|e| e.alive() && e.body.contains(x, y))
            {
                return Some(i);
            }
            wyrm.and_then(|w| w.segment_at(x, y)).map(|s| n_enemies + s)
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
            if i < n_enemies {
                self.enemies[i].struck_by(&p);
            } else if let Some(w) = &mut self.wyrm {
                w.struck_by(i - n_enemies, &p);
            }
        }
    }

    /// Blasts hurt and knock back the player and every creature.
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
            if self.player.alive() && !self.mods.blast_immune {
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
            if let Some(w) = &mut self.wyrm {
                for i in 0..w.segments.len() {
                    let s = &w.segments[i];
                    if let Some((f, _, _)) = push((s.x, s.y)) {
                        w.damage(i, 40.0 * f * strength);
                    }
                }
            }
        }
        if player_damage > 0.0 && self.phase == Phase::Playing {
            self.player
                .damage(player_damage.min(MAX_BLAST_DAMAGE_PER_TICK));
        }
    }

    /// Remove the dead, leaving remains and gold in the world.
    fn reap_creatures(&mut self) {
        let mut killed = 0;
        let mut drops = Vec::new();
        let world = &mut self.world;
        self.enemies.retain(|e| {
            if e.alive() {
                return true;
            }
            e.remains(world);
            killed += e.credited as u32;
            let gold = match e.kind {
                EnemyKind::Scarab => 3,
                EnemyKind::Wraith => 2,
                EnemyKind::Golem => 10,
            };
            drops.push((e.body.center(), gold));
            false
        });
        if self.wyrm.as_ref().is_some_and(|w| !w.alive()) {
            let w = self.wyrm.take().unwrap();
            w.remains(&mut self.world);
            // Its hoard spills out beneath the rift, where you'll pass.
            let hoard = self.rift_spot.map_or((w.head().x, w.head().y), |r| {
                let (x, _) = r.center();
                (x as f32, (r.y + r.h + 6) as f32)
            });
            drops.push((hoard, 40));
            killed += 1;
            // The way out opens where the lair's rift was foretold.
            self.exit = self.rift_spot;
        }
        for ((x, y), n) in drops {
            self.drop_gold(x, y, n);
        }
        if self.phase == Phase::Playing {
            self.kills += killed;
        }
    }

    fn drop_gold(&mut self, x: f32, y: f32, n: u32) {
        let mut placed = 0;
        for _ in 0..n * 6 {
            if placed == n {
                break;
            }
            let gx = x as i32 + self.rng.range(0, 8) as i32 - 4;
            let gy = y as i32 + self.rng.range(0, 8) as i32 - 4;
            // Gold can settle into open air or loose powder (a buried
            // creature's hoard ends up in the sand, waiting to be dug out).
            let m = self.world.get(gx, gy);
            let open =
                m == id::EMPTY || physand_sim::material::kind(m) == physand_sim::Kind::Powder;
            if open && m != id::GOLD && self.world.in_bounds(gx, gy) {
                self.world.set(gx, gy, id::GOLD);
                placed += 1;
            }
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
        let Some(exit) = self.exit else { return };
        // The rift devours any material that falls into it, so sand pouring
        // in can never bury the way out.
        for y in exit.y..exit.y + exit.h {
            for x in exit.x..exit.x + exit.w {
                if self.world.get(x, y) != id::EMPTY {
                    self.world.set(x, y, id::EMPTY);
                }
            }
        }
        let (px, py) = self.player.center();
        if !exit.contains(px as i32, py as i32) {
            return;
        }
        match self.stage {
            Stage::Floor(_) => self.phase = Phase::Cleared,
            Stage::Distillery(_) => self.leave_distillery(),
            Stage::Boss => self.phase = Phase::Victory,
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
                let mat = self.world.get(x, y);
                let chance = props(mat).dig;
                if chance <= 0.0 || !self.rng.chance(chance) || self.world.cell(x, y).is_none() {
                    continue;
                }
                if mat == id::GOLD {
                    self.gold += 1;
                    self.world.set(x, y, id::EMPTY);
                } else if self.rng.chance(self.mods.dig_gold) {
                    // Philosopher's Shard: the stone turns to gold as you dig.
                    self.world.set(x, y, id::GOLD);
                } else {
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

    /// Where the compass points: the seal until it is open, then the exit;
    /// in the lair, the wyrm until it falls.
    pub fn objective(&self) -> Option<(f32, f32)> {
        if let Some(w) = &self.wyrm {
            return Some((w.head().x, w.head().y));
        }
        let r = if self.seal_open {
            self.exit?
        } else {
            self.seal?
        };
        let (x, y) = r.center();
        Some((x as f32, y as f32))
    }

    // ---------------------------------------------------------------------
    // Rendering
    // ---------------------------------------------------------------------

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
        let here = self.station_here();
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

        // Distillery stations.
        if let Some(d) = &self.distillery {
            for (i, s) in d.stations.iter().enumerate() {
                let (sx, _) = s.zone.center();
                let bob = ((tick as f32 * 0.08 + i as f32).sin() * 1.5) as i32;
                let (x, y) = (sx - cx, s.zone.y + 4 + bob - cy);
                match s.kind {
                    StationKind::Reward(item) | StationKind::Shop(item) if !s.spent => {
                        o.icon(x, y, item, here == Some(i));
                    }
                    StationKind::Fountain if !s.spent => {
                        o.sparkle(x, y + 8, [140, 200, 255, 230], tick)
                    }
                    StationKind::Workbench => o.sparkle(x, y + 6, [255, 210, 120, 200], tick),
                    _ => {}
                }
            }
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

        if let Some(w) = &self.wyrm {
            // Tail first so the head draws on top.
            for (i, s) in w.segments.iter().enumerate().rev() {
                let r = if i == 0 { HEAD_RADIUS } else { SEGMENT_RADIUS } as i32;
                let base = if s.glassed {
                    [175, 215, 225, 235]
                } else {
                    [206, 170, 104, 255]
                };
                let heat = (s.heat / crate::wyrm::GLASS_HEAT).min(1.0);
                let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * heat * 0.7) as u8;
                let mut c = [
                    mix(base[0], 255),
                    mix(base[1], 110),
                    mix(base[2], 30),
                    base[3],
                ];
                if s.hurt_flash > 0 {
                    c = [255, 255, 255, 255];
                }
                let (x, y) = (s.x as i32 - cx, s.y as i32 - cy);
                o.disc(x, y, r, c, [90, 66, 40, 255]);
                if i == 0 {
                    // Eyes and mandibles, facing the direction of travel.
                    let next = w.segments.get(1).map_or((s.x + 1.0, s.y), |n| (n.x, n.y));
                    let (dx, dy) = (s.x - next.0, s.y - next.1);
                    let d = (dx * dx + dy * dy).sqrt().max(0.01);
                    let (ux, uy) = (dx / d, dy / d);
                    let eye = [255, 60, 30, 255];
                    for side in [-1.0, 1.0] {
                        let ex = x as f32 + ux * 2.0 - uy * side * 2.0;
                        let ey = y as f32 + uy * 2.0 + ux * side * 2.0;
                        o.put(ex.round() as i32, ey.round() as i32, eye);
                        let mx = x as f32 + ux * 6.0 - uy * side * 3.0;
                        let my = y as f32 + uy * 6.0 + ux * side * 3.0;
                        o.put(mx.round() as i32, my.round() as i32, [60, 40, 24, 255]);
                    }
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
    /// Returns `false` if the recipe is invalid, uses components you don't
    /// own, or you are not at a workbench.
    pub fn configure_flask(
        &mut self,
        index: usize,
        reagents: &[u8],
        form: u8,
        catalysts: &[u8],
    ) -> bool {
        if !self.can_edit_flasks() {
            return false;
        }
        let reagents: Vec<u8> = reagents.iter().copied().filter(|&r| r != 255).collect();
        let Some(&form) = Form::ALL.get(form as usize) else {
            return false;
        };
        let Some(catalysts) = catalysts
            .iter()
            .filter(|&&c| c != 255)
            .map(|&c| Catalyst::ALL.get(c as usize).copied())
            .collect::<Option<Vec<Catalyst>>>()
        else {
            return false;
        };
        if !self.inventory.allows(&reagents, form, &catalysts) {
            return false;
        }
        match self.flasks.get_mut(index) {
            Some(flask) => flask.configure(&reagents, form, &catalysts).is_ok(),
            None => false,
        }
    }
}
