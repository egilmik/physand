//! What a run carries between floors: owned spell components, relics, and
//! the items that can be found or bought.

use physand_sim::{
    id,
    material::{props, reagents},
    rng::Rng,
};

use crate::spell::{Catalyst, Form};

/// Floors per biome before the boss.
pub const FLOORS_PER_RUN: u32 = 3;

// ---------------------------------------------------------------------------
// Relics
// ---------------------------------------------------------------------------

/// Passive trinkets that bend the rules for the rest of the run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relic {
    EmberWard,
    GlassSkin,
    ScarabCarapace,
    LevitationCharm,
    BottomlessFlask,
    VolatileHeart,
    PhilosophersShard,
    MagnetRing,
}

impl Relic {
    pub const ALL: [Relic; 8] = [
        Relic::EmberWard,
        Relic::GlassSkin,
        Relic::ScarabCarapace,
        Relic::LevitationCharm,
        Relic::BottomlessFlask,
        Relic::VolatileHeart,
        Relic::PhilosophersShard,
        Relic::MagnetRing,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Relic::EmberWard => "Ember Ward",
            Relic::GlassSkin => "Glass Skin",
            Relic::ScarabCarapace => "Scarab Carapace",
            Relic::LevitationCharm => "Levitation Charm",
            Relic::BottomlessFlask => "Bottomless Flask",
            Relic::VolatileHeart => "Volatile Heart",
            Relic::PhilosophersShard => "Philosopher's Shard",
            Relic::MagnetRing => "Magnet Ring",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Relic::EmberWard => "Fire burns you far less",
            Relic::GlassSkin => "Acid barely touches you",
            Relic::ScarabCarapace => "+25 maximum health",
            Relic::LevitationCharm => "Levitate 60% longer",
            Relic::BottomlessFlask => "Flasks refill 50% faster",
            Relic::VolatileHeart => "Explosions cannot harm you",
            Relic::PhilosophersShard => "Some of what you dig turns to gold",
            Relic::MagnetRing => "Gold flies to you from afar",
        }
    }

    pub fn color(self) -> [u8; 3] {
        match self {
            Relic::EmberWard => [255, 120, 40],
            Relic::GlassSkin => [175, 215, 225],
            Relic::ScarabCarapace => [40, 140, 150],
            Relic::LevitationCharm => [110, 190, 230],
            Relic::BottomlessFlask => [70, 120, 230],
            Relic::VolatileHeart => [230, 50, 60],
            Relic::PhilosophersShard => [255, 210, 70],
            Relic::MagnetRing => [200, 200, 210],
        }
    }
}

/// The combined effect of the relics held.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Modifiers {
    pub fire_damage: f32,
    pub acid_damage: f32,
    pub bonus_hp: f32,
    pub stamina: f32,
    pub flask_regen: f32,
    pub blast_immune: bool,
    /// Chance that a dug cell turns into gold.
    pub dig_gold: f32,
    /// Radius (cells) from which gold is pulled in.
    pub magnet: i32,
}

impl Default for Modifiers {
    fn default() -> Self {
        Modifiers {
            fire_damage: 1.0,
            acid_damage: 1.0,
            bonus_hp: 0.0,
            stamina: 1.0,
            flask_regen: 1.0,
            blast_immune: false,
            dig_gold: 0.0,
            magnet: 0,
        }
    }
}

impl Modifiers {
    pub fn from_relics(relics: &[Relic]) -> Modifiers {
        let mut m = Modifiers::default();
        for r in relics {
            match r {
                Relic::EmberWard => m.fire_damage *= 0.3,
                Relic::GlassSkin => m.acid_damage *= 0.25,
                Relic::ScarabCarapace => m.bonus_hp += 25.0,
                Relic::LevitationCharm => m.stamina *= 1.6,
                Relic::BottomlessFlask => m.flask_regen *= 1.5,
                Relic::VolatileHeart => m.blast_immune = true,
                Relic::PhilosophersShard => m.dig_gold = 0.04,
                Relic::MagnetRing => m.magnet = 12,
            }
        }
        m
    }
}

// ---------------------------------------------------------------------------
// Items
// ---------------------------------------------------------------------------

/// Anything that can sit on a pedestal or a shop shelf.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Reagent(u8),
    Form(Form),
    Catalyst(Catalyst),
    Relic(Relic),
    /// Restore health.
    Heal,
    /// Bigger flasks.
    Capacity,
}

pub const HEAL_AMOUNT: f32 = 40.0;
pub const CAPACITY_BONUS: f32 = 100.0;

impl Item {
    pub fn name(self) -> String {
        match self {
            Item::Reagent(m) => format!("{} reagent", props(m).name),
            Item::Form(f) => format!("{} form", f.name()),
            Item::Catalyst(c) => format!("{} catalyst", c.name()),
            Item::Relic(r) => r.name().to_string(),
            Item::Heal => "Healing draught".to_string(),
            Item::Capacity => "Flask enlargement".to_string(),
        }
    }

    pub fn description(self) -> String {
        match self {
            Item::Reagent(m) => match m {
                id::ACID => "Dissolves rock, sand and flesh — not glass".into(),
                id::LAVA => "Molten rock: burns, melts, ignites".into(),
                id::ICE => "Frozen water that chills what it touches".into(),
                id::GUNPOWDER => "Explodes when heated or lit".into(),
                id::WOOD => "Solid fuel for building and burning".into(),
                id::WATER => "Douses fire, erodes sand creatures".into(),
                id::OIL => "Floats and burns".into(),
                id::SAND => "Heavy and buries things; heat makes glass".into(),
                _ => "A new material for your flasks".into(),
            },
            Item::Form(f) => match f {
                Form::Spray => "A continuous stream".into(),
                Form::Orb => "A projectile that bursts into a blob".into(),
                Form::Rain => "Falls from the ceiling above your aim".into(),
                Form::Wall => "An instant barrier across your aim".into(),
            },
            Item::Catalyst(c) => match c {
                Catalyst::Heat => "Scalding: ignites, boils, melts wax".into(),
                Catalyst::Frost => "Freezing: ice, obsidian, frozen foes".into(),
                Catalyst::Volatile => "Explodes on impact".into(),
                Catalyst::Bloom => "More material per cast".into(),
                Catalyst::Transmute => "Turns what it hits into the reagent".into(),
            },
            Item::Relic(r) => r.description().into(),
            Item::Heal => format!("Restore {HEAL_AMOUNT} health"),
            Item::Capacity => format!("+{CAPACITY_BONUS} volume in every flask"),
        }
    }

    /// Icon colour.
    pub fn color(self) -> [u8; 3] {
        match self {
            Item::Reagent(m) => props(m).color,
            Item::Form(_) => [230, 230, 240],
            Item::Catalyst(c) => match c {
                Catalyst::Heat => [255, 110, 30],
                Catalyst::Frost => [150, 210, 255],
                Catalyst::Volatile => [255, 60, 50],
                Catalyst::Bloom => [120, 230, 120],
                Catalyst::Transmute => [200, 120, 255],
            },
            Item::Relic(r) => r.color(),
            Item::Heal => [230, 50, 70],
            Item::Capacity => [80, 140, 240],
        }
    }

    /// Shop price in gold.
    pub fn price(self) -> u32 {
        match self {
            Item::Heal => 30,
            Item::Capacity => 50,
            Item::Reagent(_) => 60,
            Item::Form(_) => 70,
            Item::Catalyst(_) => 75,
            Item::Relic(_) => 90,
        }
    }
}

// ---------------------------------------------------------------------------
// Inventory
// ---------------------------------------------------------------------------

/// Spell components and relics owned this run.
#[derive(Clone, Debug, PartialEq)]
pub struct Inventory {
    pub reagents: Vec<u8>,
    pub forms: Vec<Form>,
    pub catalysts: Vec<Catalyst>,
    pub relics: Vec<Relic>,
}

impl Inventory {
    /// What a new run starts with.
    pub fn starter() -> Inventory {
        Inventory {
            reagents: vec![id::WATER, id::OIL, id::SAND],
            forms: vec![Form::Spray, Form::Orb, Form::Wall],
            catalysts: vec![Catalyst::Heat],
            relics: Vec::new(),
        }
    }

    /// Everything (sandboxes and tests).
    pub fn everything() -> Inventory {
        Inventory {
            reagents: reagents().collect(),
            forms: Form::ALL.to_vec(),
            catalysts: Catalyst::ALL.to_vec(),
            relics: Vec::new(),
        }
    }

    pub fn owns(&self, item: Item) -> bool {
        match item {
            Item::Reagent(m) => self.reagents.contains(&m),
            Item::Form(f) => self.forms.contains(&f),
            Item::Catalyst(c) => self.catalysts.contains(&c),
            Item::Relic(r) => self.relics.contains(&r),
            Item::Heal | Item::Capacity => false,
        }
    }

    /// Add a component or relic (consumables are applied by the game).
    pub fn add(&mut self, item: Item) {
        if self.owns(item) {
            return;
        }
        match item {
            Item::Reagent(m) => self.reagents.push(m),
            Item::Form(f) => self.forms.push(f),
            Item::Catalyst(c) => self.catalysts.push(c),
            Item::Relic(r) => self.relics.push(r),
            Item::Heal | Item::Capacity => {}
        }
    }

    /// Whether a recipe uses only owned components.
    pub fn allows(&self, reagents: &[u8], form: Form, catalysts: &[Catalyst]) -> bool {
        reagents.iter().all(|r| self.reagents.contains(r))
            && self.forms.contains(&form)
            && catalysts.iter().all(|c| self.catalysts.contains(c))
    }

    pub fn modifiers(&self) -> Modifiers {
        Modifiers::from_relics(&self.relics)
    }

    /// Every component and relic not yet owned.
    pub fn unowned(&self) -> Vec<Item> {
        let mut pool: Vec<Item> = Vec::new();
        pool.extend(reagents().map(Item::Reagent));
        pool.extend(Form::ALL.iter().map(|&f| Item::Form(f)));
        pool.extend(Catalyst::ALL.iter().map(|&c| Item::Catalyst(c)));
        pool.extend(Relic::ALL.iter().map(|&r| Item::Relic(r)));
        pool.retain(|&i| !self.owns(i));
        pool
    }
}

/// Pick `n` distinct items the player doesn't own yet, excluding `taken`.
/// When the pool runs dry, consumables fill the gaps.
pub fn roll_items(inv: &Inventory, rng: &mut Rng, n: usize, taken: &[Item]) -> Vec<Item> {
    let mut pool = inv.unowned();
    pool.retain(|i| !taken.contains(i));
    let mut out = Vec::with_capacity(n);
    while out.len() < n && !pool.is_empty() {
        let i = (rng.next_u32() as usize) % pool.len();
        out.push(pool.swap_remove(i));
    }
    let fillers = [Item::Heal, Item::Capacity];
    let mut k = 0;
    while out.len() < n {
        out.push(fillers[k % 2]);
        k += 1;
    }
    out
}
