//! Palette indices shared by the models and the client's hull shader (which holds the colours).

pub const WHITE: u8 = 0;
pub const BLUE: u8 = 1;
pub const RED: u8 = 2;
pub const YELLOW: u8 = 3;
/// Joints, hands and the inner frame.
pub const DARK: u8 = 4;
pub const OZ_GREEN: u8 = 5;
pub const OZ_GREY: u8 = 6;
pub const TAURUS_WHITE: u8 = 7;
pub const TAURUS_BLUE: u8 = 8;
pub const VIRGO_OLIVE: u8 = 9;
pub const ALLIANCE_TAN: u8 = 10;
/// Colony hull plating.
pub const HULL: u8 = 11;
/// Colony hull, darker service plating.
pub const HULL_DARK: u8 = 12;
/// The colony's mirrors: aluminised film on a frame.
pub const MIRROR: u8 = 13;
/// Weapons.
pub const GUNMETAL: u8 = 14;
/// Visors and sensor glass.
pub const GLASS: u8 = 15;

/// How many there are (the palette's size).
pub const COUNT: usize = 16;
