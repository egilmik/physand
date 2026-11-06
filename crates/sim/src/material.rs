//! Material definitions: ids, physical class, density, colour and thermal
//! behaviour.
//!
//! The table here is the single source of truth; the web front end reads
//! names and colours from it through the wasm bindings.

/// Material identifiers. Stored as the first byte of every [`crate::Cell`].
pub mod id {
    pub const EMPTY: u8 = 0;
    pub const ROCK: u8 = 1;
    pub const SAND: u8 = 2;
    pub const WATER: u8 = 3;
    pub const FIRE: u8 = 4;
    pub const STEAM: u8 = 5;
    pub const WOOD: u8 = 6;
    pub const OIL: u8 = 7;
    pub const LAVA: u8 = 8;
    pub const OBSIDIAN: u8 = 9;
    pub const SMOKE: u8 = 10;
    pub const ACID: u8 = 11;
    pub const ICE: u8 = 12;
    pub const GUNPOWDER: u8 = 13;
    pub const GLASS: u8 = 14;
    pub const SANDSTONE: u8 = 15;
    pub const WAX: u8 = 16;
}

/// Temperature of the cave air. Temperatures are abstract `u8` units:
/// water freezes at 20 and boils at 100, lava sits at 250.
pub const AMBIENT: u8 = 40;

/// How a material moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Empty,
    /// Never moves.
    Solid,
    /// Falls, piles up diagonally.
    Powder,
    /// Falls and spreads horizontally.
    Liquid,
    /// Rises and drifts.
    Gas,
}

/// What a cell turns into, used by phase changes and reactions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    /// Stay as is.
    Keep,
    /// Become another material.
    Become(u8),
    /// Become another material with the given probability, else stay.
    BecomeChance(u8, f32),
    /// Become another material at that material's base temperature (a phase
    /// change normally keeps the temperature).
    BecomeFresh(u8),
    /// Catch fire as long-burning fuel (stationary embers).
    Ignite,
    /// Detonate.
    Explode,
}

#[derive(Clone, Copy, Debug)]
pub struct Props {
    pub name: &'static str,
    pub kind: Kind,
    /// Heavier materials sink through lighter liquids and gases.
    pub density: u8,
    /// Base colour (sRGB).
    pub color: [u8; 3],
    /// Lifetime range in ticks for transient materials (`0` = immortal).
    pub life: (u8, u8),
    /// Max horizontal cells a liquid may flow per tick.
    pub spread: u8,
    /// Per-tick chance that a cell is removed while being dug (0 = undiggable).
    pub dig: f32,
    /// Temperature a freshly created cell has.
    pub base_temp: u8,
    /// How readily heat flows to/from neighbours (0 = insulator).
    pub conduct: f32,
    /// Chance per tick of drifting toward ambient when next to open air.
    pub air_cool: f32,
    /// Transition when temperature rises to at least the threshold.
    pub on_hot: Option<(u8, Outcome)>,
    /// Transition when temperature falls to at most the threshold.
    pub on_cold: Option<(u8, Outcome)>,
    /// Whether this material can be cast from a flask.
    pub reagent: bool,
}

const BASE: Props = Props {
    name: "",
    kind: Kind::Solid,
    density: 255,
    color: [255, 0, 255],
    life: (0, 0),
    spread: 0,
    dig: 0.0,
    base_temp: AMBIENT,
    conduct: 0.0,
    air_cool: 0.0,
    on_hot: None,
    on_cold: None,
    reagent: false,
};

pub const MATERIALS: [Props; 17] = [
    Props {
        name: "Empty",
        kind: Kind::Empty,
        density: 0,
        color: [0, 0, 0],
        ..BASE
    },
    // Rock is a thermal insulator so settled caves around lava can sleep.
    Props {
        name: "Rock",
        color: [92, 88, 96],
        dig: 0.04,
        ..BASE
    },
    Props {
        name: "Sand",
        kind: Kind::Powder,
        density: 90,
        color: [214, 182, 110],
        dig: 0.5,
        conduct: 0.25,
        air_cool: 0.25,
        on_hot: Some((210, Outcome::Become(id::GLASS))),
        reagent: true,
        ..BASE
    },
    Props {
        name: "Water",
        kind: Kind::Liquid,
        density: 50,
        color: [48, 110, 210],
        spread: 5,
        conduct: 0.35,
        air_cool: 0.25,
        on_hot: Some((100, Outcome::Become(id::STEAM))),
        on_cold: Some((20, Outcome::Become(id::ICE))),
        reagent: true,
        ..BASE
    },
    Props {
        name: "Fire",
        kind: Kind::Gas,
        density: 2,
        color: [255, 140, 40],
        life: (20, 60),
        base_temp: 200,
        conduct: 0.3,
        ..BASE
    },
    Props {
        name: "Steam",
        kind: Kind::Gas,
        density: 1,
        color: [200, 205, 215],
        life: (120, 240),
        base_temp: 110,
        ..BASE
    },
    Props {
        name: "Wood",
        color: [120, 78, 44],
        dig: 0.15,
        conduct: 0.1,
        air_cool: 0.2,
        on_hot: Some((170, Outcome::Ignite)),
        reagent: true,
        ..BASE
    },
    Props {
        name: "Oil",
        kind: Kind::Liquid,
        density: 40,
        color: [70, 52, 40],
        spread: 4,
        conduct: 0.3,
        air_cool: 0.25,
        on_hot: Some((150, Outcome::Ignite)),
        reagent: true,
        ..BASE
    },
    Props {
        name: "Lava",
        kind: Kind::Liquid,
        density: 80,
        color: [235, 80, 20],
        spread: 1,
        base_temp: 250,
        // Holds a finite amount of heat and gives it up slowly, so lava can
        // bake nearby sand into glass without an endless heat front.
        conduct: 0.12,
        on_cold: Some((140, Outcome::Become(id::OBSIDIAN))),
        reagent: true,
        ..BASE
    },
    // An insulator: a crust of obsidian seals a lava pool instead of
    // wicking its heat away.
    Props {
        name: "Obsidian",
        color: [40, 30, 56],
        ..BASE
    },
    Props {
        name: "Smoke",
        kind: Kind::Gas,
        density: 1,
        color: [70, 68, 72],
        life: (40, 90),
        ..BASE
    },
    Props {
        name: "Acid",
        kind: Kind::Liquid,
        density: 55,
        color: [120, 230, 60],
        spread: 4,
        conduct: 0.3,
        air_cool: 0.25,
        reagent: true,
        ..BASE
    },
    Props {
        name: "Ice",
        color: [170, 215, 245],
        dig: 0.3,
        base_temp: 0,
        conduct: 0.35,
        air_cool: 0.05,
        on_hot: Some((32, Outcome::Become(id::WATER))),
        reagent: true,
        ..BASE
    },
    Props {
        name: "Gunpowder",
        kind: Kind::Powder,
        density: 70,
        color: [60, 60, 66],
        dig: 0.5,
        conduct: 0.25,
        air_cool: 0.25,
        on_hot: Some((110, Outcome::Explode)),
        reagent: true,
        ..BASE
    },
    // Glass resists acid and makes see-through walls (sand + heat).
    Props {
        name: "Glass",
        color: [175, 215, 225],
        dig: 0.08,
        conduct: 0.2,
        air_cool: 0.25,
        ..BASE
    },
    // Catacomb masonry: softer than rock, an insulator, eaten by acid.
    Props {
        name: "Sandstone",
        color: [184, 140, 92],
        dig: 0.12,
        ..BASE
    },
    // Ancient seal over each floor's descent. Nothing breaks it except heat,
    // which melts it into oil.
    Props {
        name: "Wax",
        color: [178, 48, 52],
        conduct: 0.3,
        air_cool: 0.1,
        // Molten wax is plain, cool oil: it only burns if you bring fire.
        on_hot: Some((100, Outcome::BecomeFresh(id::OIL))),
        ..BASE
    },
];

#[inline]
pub fn props(mat: u8) -> &'static Props {
    &MATERIALS[mat as usize]
}

#[inline]
pub fn kind(mat: u8) -> Kind {
    props(mat).kind
}

/// Whether a moving `mover` may swap into a cell holding `target`.
#[inline]
pub fn can_displace(mover: u8, target: u8) -> bool {
    if target == id::EMPTY {
        return true;
    }
    let (m, t) = (props(mover), props(target));
    m.kind != Kind::Gas && matches!(t.kind, Kind::Liquid | Kind::Gas) && t.density < m.density
}

/// Ids of materials that can be loaded into a flask.
pub fn reagents() -> impl Iterator<Item = u8> {
    (0..MATERIALS.len() as u8).filter(|&m| props(m).reagent)
}
