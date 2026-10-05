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

/// The Oz suits' inner frame: the Leo's joints, hands and feet, a dark olive brown. The first 16
/// entries are what a livery can name (a hull tag has four bits for each of its paints); from here
/// on, only a model's fixed paints reach them.
pub const FRAME_BROWN: u8 = 16;

/// How many there are (the palette's size).
pub const COUNT: usize = 17;
/// The most the hull shader's palette holds.
pub const CAPACITY: usize = 32;
const _: () = assert!(COUNT <= CAPACITY);
