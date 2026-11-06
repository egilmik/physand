//! The Distillery's stations. They are entities rather than cells, so spells
//! tried out in the room can never destroy them.

use physand_sim::{
    rng::Rng,
    worldgen::{Level, Marker, Rect},
};

use crate::run::{roll_items, Inventory, Item};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StationKind {
    /// Pick one of three; the others vanish.
    Reward(Item),
    /// Buy for gold.
    Shop(Item),
    /// One free refill.
    Fountain,
    /// Edit flasks here.
    Workbench,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Station {
    pub kind: StationKind,
    pub zone: Rect,
    /// Used up (taken, bought, drunk from).
    pub spent: bool,
}

/// What happened when the player pressed interact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interaction {
    Took(Item),
    Bought(Item),
    TooPoor { price: u32 },
    Refilled,
    AlreadyUsed,
    OpenSpellcraft,
}

/// What the HUD shows when standing at a station.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prompt {
    pub title: String,
    pub detail: String,
    pub price: Option<u32>,
    pub affordable: bool,
}

#[derive(Clone, Debug)]
pub struct Distillery {
    pub stations: Vec<Station>,
}

impl Distillery {
    /// Stock the stations for a level's markers.
    pub fn stock(level: &Level, inv: &Inventory, rng: &mut Rng) -> Distillery {
        let rewards = roll_items(inv, rng, level.marked(Marker::Pedestal).len(), &[]);
        let special = roll_items(inv, rng, 1, &rewards)[0];
        let shop_items = [Item::Heal, Item::Capacity, special];
        let mut stations = Vec::new();
        for (zone, item) in level.marked(Marker::Pedestal).into_iter().zip(rewards) {
            stations.push(Station {
                kind: StationKind::Reward(item),
                zone,
                spent: false,
            });
        }
        for (zone, item) in level.marked(Marker::ShopSlot).into_iter().zip(shop_items) {
            stations.push(Station {
                kind: StationKind::Shop(item),
                zone,
                spent: false,
            });
        }
        for zone in level.marked(Marker::Fountain) {
            stations.push(Station {
                kind: StationKind::Fountain,
                zone,
                spent: false,
            });
        }
        for zone in level.marked(Marker::Workbench) {
            stations.push(Station {
                kind: StationKind::Workbench,
                zone,
                spent: false,
            });
        }
        Distillery { stations }
    }

    /// The live station whose zone contains the point, if any.
    pub fn at(&self, x: f32, y: f32) -> Option<usize> {
        self.stations.iter().position(|s| {
            let live = !s.spent || matches!(s.kind, StationKind::Workbench | StationKind::Fountain);
            live && s.zone.contains(x as i32, y as i32)
        })
    }

    /// Mark a reward as taken; the other rewards vanish with it.
    pub fn take_reward(&mut self) {
        for s in &mut self.stations {
            if matches!(s.kind, StationKind::Reward(_)) {
                s.spent = true;
            }
        }
    }

    pub fn prompt(&self, index: usize, gold: u32) -> Prompt {
        let s = &self.stations[index];
        match s.kind {
            StationKind::Reward(item) => Prompt {
                title: format!("Take {}", item.name()),
                detail: item.description(),
                price: None,
                affordable: true,
            },
            StationKind::Shop(item) => Prompt {
                title: format!("Buy {}", item.name()),
                detail: item.description(),
                price: Some(item.price()),
                affordable: gold >= item.price(),
            },
            StationKind::Fountain => Prompt {
                title: if s.spent {
                    "The fountain is dry".into()
                } else {
                    "Drink from the fountain".into()
                },
                detail: "Refills every flask and restores some health".into(),
                price: None,
                affordable: !s.spent,
            },
            StationKind::Workbench => Prompt {
                title: "Use the workbench".into(),
                detail: "Rework your flasks with the components you own".into(),
                price: None,
                affordable: true,
            },
        }
    }
}
