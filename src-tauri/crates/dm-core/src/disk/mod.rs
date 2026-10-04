//! Disks and partitions: read the layout, explain problems, run safe fixes.

pub mod actions;
pub mod diagnose;
pub mod model;

pub use actions::Action;
pub use diagnose::{diagnose, Finding};
pub use model::{read, DiskView, Layout};

/// Drive letter Windows runs from.
pub fn windows_letter() -> char {
    crate::drives::system_root().chars().next().unwrap_or('C')
}

/// Letters taken by any drive, including network drives and CD readers.
pub fn letters_in_use(layout: &Layout) -> Vec<char> {
    let mut out: Vec<char> = crate::drives::list()
        .iter()
        .filter_map(|d| d.root.chars().next())
        .chain(
            layout
                .volumes
                .iter()
                .filter_map(|v| v.letter.chars().next()),
        )
        .map(|c| c.to_ascii_uppercase())
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}
