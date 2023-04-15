#![allow(dead_code)]
pub const BASIC_CONTROL_REG: u16 = 0;
pub const BASIC_CONTROL_REG_DUPLEX_MODE: u16 = 1 << 8;
pub const BASIC_CONTROL_REG_REST_AUTO_NEG: u16 = 1 << 9;
pub const BASIC_STATUS_REG: u16 = 1;
pub const BASIC_STATUS_REG_LINK_STATUS: u16 = 1 << 2;
pub const BASIC_STATUS_REG_AUTO_NEGO_COMPLETE: u16 = 1 << 5;
pub const AUTO_NEGO_REG: u16 = 4;
pub const AUTO_NEGO_REG_IEEE802_3: u16 = 0b00001 << 0;
pub const AUTO_NEGO_REG_10_ABI: u16 = 1 << 5;
pub const AUTO_NEGO_REG_10_FD_ABI: u16 = 1 << 6;
pub const AUTO_NEGO_REG_100_ABI: u16 = 1 << 7;
pub const AUTO_NEGO_REG_100_FD_ABI: u16 = 1 << 8;
