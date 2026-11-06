//! Pairwise material reactions, as data.
//!
//! When a cell checks a neighbour, the table is consulted for
//! `(self, neighbour)`; if there is no entry, the reverse pair is tried with
//! roles swapped. Adding a combo means adding a row here, not new code.

use crate::material::{id, Outcome, MATERIALS};

#[derive(Clone, Copy, Debug)]
pub struct Reaction {
    pub a: u8,
    pub b: u8,
    /// Chance per check that the reaction fires.
    pub chance: f32,
    pub a_to: Outcome,
    pub b_to: Outcome,
}

const fn r(a: u8, b: u8, chance: f32, a_to: Outcome, b_to: Outcome) -> Reaction {
    Reaction {
        a,
        b,
        chance,
        a_to,
        b_to,
    }
}

use Outcome::{Become, BecomeChance, Explode, Ignite, Keep};

pub const REACTIONS: &[Reaction] = &[
    // Lava
    r(
        id::LAVA,
        id::WATER,
        1.0,
        Become(id::OBSIDIAN),
        Become(id::STEAM),
    ),
    r(
        id::LAVA,
        id::ICE,
        0.5,
        Become(id::OBSIDIAN),
        Become(id::WATER),
    ),
    r(id::LAVA, id::WOOD, 0.1, Keep, Ignite),
    r(id::LAVA, id::OIL, 0.1, Keep, Ignite),
    r(id::LAVA, id::GUNPOWDER, 1.0, Keep, Explode),
    // Fire (fire checks all four neighbours, so chances are per neighbour)
    r(id::FIRE, id::WOOD, 0.08, Keep, Ignite),
    r(id::FIRE, id::OIL, 0.3, Keep, Ignite),
    r(id::FIRE, id::GUNPOWDER, 0.5, Keep, Explode),
    r(
        id::FIRE,
        id::WATER,
        1.0,
        Become(id::STEAM),
        BecomeChance(id::STEAM, 0.2),
    ),
    r(id::FIRE, id::ICE, 0.05, Keep, Become(id::WATER)),
    r(id::FIRE, id::ACID, 0.05, Keep, Become(id::SMOKE)),
    // Acid eats most things, but not glass or obsidian, and is used up doing so.
    r(
        id::ACID,
        id::ROCK,
        0.05,
        BecomeChance(id::SMOKE, 0.3),
        Become(id::EMPTY),
    ),
    r(
        id::ACID,
        id::SAND,
        0.1,
        BecomeChance(id::SMOKE, 0.3),
        Become(id::EMPTY),
    ),
    r(
        id::ACID,
        id::WOOD,
        0.1,
        BecomeChance(id::SMOKE, 0.3),
        Become(id::EMPTY),
    ),
    r(
        id::ACID,
        id::ICE,
        0.1,
        BecomeChance(id::SMOKE, 0.3),
        Become(id::WATER),
    ),
    r(
        id::ACID,
        id::GUNPOWDER,
        0.1,
        BecomeChance(id::SMOKE, 0.3),
        Become(id::EMPTY),
    ),
    r(id::ACID, id::WATER, 0.01, Become(id::WATER), Keep),
];

const N: usize = MATERIALS.len();

/// `LOOKUP[a][b]` = index into [`REACTIONS`] + 1 (0 = none).
static LOOKUP: [[u8; N]; N] = build_lookup();

const fn build_lookup() -> [[u8; N]; N] {
    let mut t = [[0u8; N]; N];
    let mut i = 0;
    while i < REACTIONS.len() {
        let rx = &REACTIONS[i];
        t[rx.a as usize][rx.b as usize] = (i + 1) as u8;
        i += 1;
    }
    t
}

/// Find the reaction between `a` and its neighbour `b`. Returns the outcome
/// for `a`, the outcome for `b`, and the chance.
#[inline]
pub fn lookup(a: u8, b: u8) -> Option<(Outcome, Outcome, f32)> {
    let fwd = LOOKUP[a as usize][b as usize];
    if fwd != 0 {
        let rx = &REACTIONS[fwd as usize - 1];
        return Some((rx.a_to, rx.b_to, rx.chance));
    }
    let rev = LOOKUP[b as usize][a as usize];
    if rev != 0 {
        let rx = &REACTIONS[rev as usize - 1];
        return Some((rx.b_to, rx.a_to, rx.chance));
    }
    None
}

/// Whether fire clings to this material (it is something that burns).
#[inline]
pub fn is_fuel(mat: u8) -> bool {
    matches!(mat, id::WOOD | id::OIL | id::GUNPOWDER)
}
