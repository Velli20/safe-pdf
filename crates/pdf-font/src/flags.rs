use bitflags::bitflags;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct FontFlags: u32 {
        const FIXED_PITCH  = 1 << 0;
        const SERIF        = 1 << 1;
        const SYMBOLIC     = 1 << 2;
        const SCRIPT       = 1 << 3;
        const NON_SYMBOLIC = 1 << 4;
        const ITALIC       = 1 << 5;
        const ALL_CAP      = 1 << 6;
        const SMALL_CAP    = 1 << 7;
        const FORCE_BOLD   = 1 << 8;
    }
}
