/// Player intent for one tick. Aim is in world cell coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Input {
    pub left: bool,
    pub right: bool,
    /// Jump when grounded; hold in the air to levitate, or in liquid to swim up.
    pub jump: bool,
    pub cast: bool,
    pub dig: bool,
    /// Use the station you are standing at (pressed, not held).
    pub interact: bool,
    pub aim_x: f32,
    pub aim_y: f32,
}

impl Input {
    pub const LEFT: u32 = 1;
    pub const RIGHT: u32 = 2;
    pub const JUMP: u32 = 4;
    pub const CAST: u32 = 8;
    pub const DIG: u32 = 16;
    pub const INTERACT: u32 = 32;

    /// Decode the compact bitmask used across the wasm boundary.
    pub fn from_bits(bits: u32, aim_x: f32, aim_y: f32) -> Input {
        Input {
            left: bits & Self::LEFT != 0,
            right: bits & Self::RIGHT != 0,
            jump: bits & Self::JUMP != 0,
            cast: bits & Self::CAST != 0,
            dig: bits & Self::DIG != 0,
            interact: bits & Self::INTERACT != 0,
            aim_x,
            aim_y,
        }
    }
}
