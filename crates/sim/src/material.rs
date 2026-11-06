//! Material definitions: ids, physical class, density and base colour.
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
}

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
}

const fn p(
    name: &'static str,
    kind: Kind,
    density: u8,
    color: [u8; 3],
    life: (u8, u8),
    spread: u8,
) -> Props {
    Props {
        name,
        kind,
        density,
        color,
        life,
        spread,
    }
}

pub const MATERIALS: [Props; 11] = [
    p("Empty", Kind::Empty, 0, [0, 0, 0], (0, 0), 0),
    p("Rock", Kind::Solid, 255, [92, 88, 96], (0, 0), 0),
    p("Sand", Kind::Powder, 90, [214, 182, 110], (0, 0), 0),
    p("Water", Kind::Liquid, 50, [48, 110, 210], (0, 0), 5),
    p("Fire", Kind::Gas, 2, [255, 140, 40], (20, 60), 0),
    p("Steam", Kind::Gas, 1, [200, 205, 215], (120, 240), 0),
    p("Wood", Kind::Solid, 255, [120, 78, 44], (0, 0), 0),
    p("Oil", Kind::Liquid, 40, [70, 52, 40], (0, 0), 4),
    p("Lava", Kind::Liquid, 80, [235, 80, 20], (0, 0), 1),
    p("Obsidian", Kind::Solid, 255, [40, 30, 56], (0, 0), 0),
    p("Smoke", Kind::Gas, 1, [70, 68, 72], (40, 90), 0),
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
