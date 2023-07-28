#![no_std]

mod arch;
mod entry;

pub use arch::Arch;
pub use entry::main;
