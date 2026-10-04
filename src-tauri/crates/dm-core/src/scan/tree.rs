//! The size tree both scanners produce.
//!
//! Scanners emit flat [`RawEntry`] records (a key, the parent's key, a name,
//! a size); [`Tree::build`] links them, adds folder sizes up from the leaves,
//! and keeps the largest files and per-extension totals. The UI then asks for
//! one folder's children at a time, so a million files never cross IPC.

use serde::Serialize;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::path::{Path, PathBuf};

/// One file or folder as a scanner found it.
#[derive(Debug, Clone)]
pub struct RawEntry {
    pub key: u64,
    pub parent: u64,
    pub name: String,
    /// Logical size in bytes (0 for folders; their size is summed).
    pub size: u64,
    pub is_dir: bool,
}

#[derive(Debug, Clone)]
struct Node {
    name: Box<str>,
    parent: u32,
    /// File: its size. Folder: everything below it.
    size: u64,
    /// Files below (1 for a file).
    files: u64,
    is_dir: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanMethod {
    /// Read the NTFS master file table directly.
    Mft,
    /// Walked folder by folder.
    Walk,
}

#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub root: PathBuf,
    pub method: ScanMethod,
    pub files: u64,
    pub folders: u64,
    pub bytes: u64,
    pub millis: u64,
    /// Folders that could not be read (walk) or records that could not be parsed (MFT).
    pub skipped: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Item {
    pub id: u32,
    pub name: String,
    pub size: u64,
    pub files: u64,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Listing {
    pub id: u32,
    pub path: PathBuf,
    /// Ancestors from the root down to this folder, for the breadcrumb.
    pub trail: Vec<Item>,
    pub size: u64,
    pub files: u64,
    pub children: Vec<Item>,
    /// Children past `limit`, rolled into one line.
    pub rest_count: u64,
    pub rest_size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BigFile {
    pub id: u32,
    pub path: PathBuf,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TypeTotal {
    /// Lower-case extension without the dot; "" for files without one.
    pub ext: String,
    pub size: u64,
    pub files: u64,
}

pub struct Tree {
    nodes: Vec<Node>,
    children: Vec<Vec<u32>>,
    top_files: Vec<u32>,
    types: Vec<TypeTotal>,
    pub summary: Summary,
}

const ROOT: u32 = 0;
const TOP_FILES: usize = 100;

impl Tree {
    /// Link `entries` under `root_key`. Entries whose parent is missing (or
    /// that sit in a parent cycle) are attached to the root so their bytes
    /// still count.
    pub fn build(
        root: PathBuf,
        root_key: u64,
        entries: Vec<RawEntry>,
        method: ScanMethod,
        millis: u64,
        skipped: u64,
    ) -> Self {
        let mut index: HashMap<u64, u32> = HashMap::with_capacity(entries.len() + 1);
        let mut nodes = Vec::with_capacity(entries.len() + 1);
        nodes.push(Node {
            name: root.to_string_lossy().into(),
            parent: ROOT,
            size: 0,
            files: 0,
            is_dir: true,
        });
        index.insert(root_key, ROOT);
        let mut parents = Vec::with_capacity(entries.len());
        for e in entries {
            if e.key == root_key || index.contains_key(&e.key) {
                continue; // the root itself, or a duplicate record
            }
            index.insert(e.key, nodes.len() as u32);
            parents.push(e.parent);
            nodes.push(Node {
                name: e.name.into(),
                parent: ROOT,
                size: if e.is_dir { 0 } else { e.size },
                files: u64::from(!e.is_dir),
                is_dir: e.is_dir,
            });
        }
        let mut children = vec![Vec::new(); nodes.len()];
        for (i, pkey) in parents.into_iter().enumerate() {
            let id = (i + 1) as u32;
            let parent = match index.get(&pkey) {
                Some(&p) if nodes[p as usize].is_dir && p != id => p,
                _ => ROOT,
            };
            nodes[id as usize].parent = parent;
            children[parent as usize].push(id);
        }

        // Post-order from the root: only reachable nodes are summed, so a
        // corrupt parent cycle can't loop forever.
        let mut order = Vec::with_capacity(nodes.len());
        let mut stack = vec![ROOT];
        let mut reached = vec![false; nodes.len()];
        reached[0] = true;
        while let Some(n) = stack.pop() {
            order.push(n);
            for &c in &children[n as usize] {
                if !reached[c as usize] {
                    reached[c as usize] = true;
                    stack.push(c);
                }
            }
        }
        // Cycle members never reached: hang them off the root.
        for id in 1..nodes.len() as u32 {
            if !reached[id as usize] {
                let old = nodes[id as usize].parent as usize;
                children[old].retain(|&c| c != id);
                nodes[id as usize].parent = ROOT;
                children[0].push(id);
                reached[id as usize] = true;
                order.push(id);
            }
        }
        for &n in order.iter().rev() {
            if n == ROOT {
                continue;
            }
            let (size, files, parent) = {
                let node = &nodes[n as usize];
                (node.size, node.files, node.parent)
            };
            let p = &mut nodes[parent as usize];
            p.size += size;
            p.files += files;
        }
        for list in &mut children {
            list.sort_unstable_by_key(|&c| Reverse(nodes[c as usize].size));
        }

        let mut heap: BinaryHeap<Reverse<(u64, u32)>> = BinaryHeap::with_capacity(TOP_FILES + 1);
        let mut types: HashMap<String, (u64, u64)> = HashMap::new();
        let mut folders = 0u64;
        for (id, n) in nodes.iter().enumerate() {
            if n.is_dir {
                folders += 1;
                continue;
            }
            heap.push(Reverse((n.size, id as u32)));
            if heap.len() > TOP_FILES {
                heap.pop();
            }
            let t = types.entry(extension(&n.name)).or_default();
            t.0 += n.size;
            t.1 += 1;
        }
        let mut top: Vec<(u64, u32)> = heap.into_iter().map(|Reverse(x)| x).collect();
        top.sort_unstable_by(|a, b| b.cmp(a));
        let mut types: Vec<TypeTotal> = types
            .into_iter()
            .map(|(ext, (size, files))| TypeTotal { ext, size, files })
            .collect();
        types.sort_unstable_by_key(|t| Reverse(t.size));

        let summary = Summary {
            root,
            method,
            files: nodes[0].files,
            folders: folders.saturating_sub(1),
            bytes: nodes[0].size,
            millis,
            skipped,
        };
        Tree {
            nodes,
            children,
            top_files: top.into_iter().map(|(_, id)| id).collect(),
            types,
            summary,
        }
    }

    pub fn root_id(&self) -> u32 {
        ROOT
    }

    /// Full path of a node.
    pub fn path(&self, id: u32) -> Option<PathBuf> {
        let mut names = Vec::new();
        let mut cur = id;
        while cur != ROOT {
            let n = self.nodes.get(cur as usize)?;
            names.push(&*n.name);
            cur = n.parent;
            if names.len() > 4096 {
                return None; // defensive: no real path is this deep
            }
        }
        let mut p = self.summary.root.clone();
        for name in names.iter().rev() {
            p.push(name);
        }
        Some(p)
    }

    fn item(&self, id: u32) -> Item {
        let n = &self.nodes[id as usize];
        Item {
            id,
            name: n.name.to_string(),
            size: n.size,
            files: n.files,
            is_dir: n.is_dir,
        }
    }

    /// One folder and its biggest `limit` children.
    pub fn listing(&self, id: u32, limit: usize) -> Option<Listing> {
        let node = self.nodes.get(id as usize)?;
        if !node.is_dir {
            return None;
        }
        let kids = &self.children[id as usize];
        let shown = kids.len().min(limit);
        let rest = &kids[shown..];
        let mut trail = Vec::new();
        let mut cur = id;
        while cur != ROOT {
            trail.push(self.item(cur));
            cur = self.nodes[cur as usize].parent;
        }
        trail.push(self.item(ROOT));
        trail.reverse();
        Some(Listing {
            id,
            path: self.path(id)?,
            trail,
            size: node.size,
            files: node.files,
            children: kids[..shown].iter().map(|&c| self.item(c)).collect(),
            rest_count: rest.len() as u64,
            rest_size: rest.iter().map(|&c| self.nodes[c as usize].size).sum(),
        })
    }

    pub fn top_files(&self, n: usize) -> Vec<BigFile> {
        self.top_files
            .iter()
            .take(n)
            .filter_map(|&id| {
                Some(BigFile {
                    id,
                    path: self.path(id)?,
                    size: self.nodes[id as usize].size,
                })
            })
            .collect()
    }

    pub fn types(&self, n: usize) -> Vec<TypeTotal> {
        self.types.iter().take(n).cloned().collect()
    }

    /// Size of a folder given by path (case-insensitive), e.g. a user profile.
    pub fn size_of(&self, path: &Path) -> Option<u64> {
        let root = self.summary.root.to_string_lossy();
        let p = path.to_string_lossy();
        // Roots are drive paths like "C:\", so the prefix is plain ASCII.
        if p.len() < root.len() || !p[..root.len()].eq_ignore_ascii_case(&root) {
            return None;
        }
        let mut cur = ROOT;
        for part in p[root.len()..].split(['\\', '/']).filter(|s| !s.is_empty()) {
            cur = *self.children[cur as usize]
                .iter()
                .find(|&&c| self.nodes[c as usize].name.eq_ignore_ascii_case(part))?;
        }
        Some(self.nodes[cur as usize].size)
    }
}

fn extension(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && ext.len() <= 12 => ext.to_ascii_lowercase(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(key: u64, parent: u64, name: &str, size: u64, is_dir: bool) -> RawEntry {
        RawEntry {
            key,
            parent,
            name: name.into(),
            size,
            is_dir,
        }
    }

    fn sample() -> Tree {
        // root(5) ── Users(10) ── a.mp4 (500), b.txt (20)
        //          └ Windows(11) ── c.dll (300)
        //          └ loose.log (7)
        //          orphan.bin (40) has a parent that doesn't exist
        let entries = vec![
            e(10, 5, "Users", 0, true),
            e(11, 5, "Windows", 0, true),
            e(20, 10, "a.mp4", 500, false),
            e(21, 10, "b.txt", 20, false),
            e(22, 11, "c.dll", 300, false),
            e(23, 5, "loose.log", 7, false),
            e(24, 999, "orphan.bin", 40, false),
        ];
        Tree::build(PathBuf::from("C:\\"), 5, entries, ScanMethod::Mft, 1, 0)
    }

    #[test]
    fn sizes_add_up_and_children_are_sorted() {
        let t = sample();
        assert_eq!(t.summary.bytes, 867);
        assert_eq!(t.summary.files, 5);
        assert_eq!(t.summary.folders, 2);
        let root = t.listing(t.root_id(), 10).unwrap();
        let names: Vec<_> = root.children.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Users", "Windows", "orphan.bin", "loose.log"]);
        assert_eq!(root.children[0].size, 520);
    }

    #[test]
    fn limit_rolls_up_the_rest() {
        let t = sample();
        let root = t.listing(t.root_id(), 2).unwrap();
        assert_eq!(root.children.len(), 2);
        assert_eq!(root.rest_count, 2);
        assert_eq!(root.rest_size, 47);
    }

    #[test]
    fn paths_trail_and_top_files() {
        let t = sample();
        let top = t.top_files(2);
        assert_eq!(top[0].path, PathBuf::from("C:\\Users\\a.mp4"));
        assert_eq!(top[1].size, 300);
        let users = t.listing(t.root_id(), 10).unwrap().children[0].id;
        let l = t.listing(users, 10).unwrap();
        assert_eq!(l.trail.len(), 2);
        assert_eq!(l.path, PathBuf::from("C:\\Users"));
        assert!(t.listing(top[0].id, 10).is_none(), "files have no listing");
    }

    #[test]
    fn types_and_size_lookup() {
        let t = sample();
        let types = t.types(10);
        assert_eq!(types[0].ext, "mp4");
        assert_eq!(t.size_of(Path::new("C:\\users")), Some(520));
        assert_eq!(t.size_of(Path::new("C:\\Nope")), None);
    }

    #[test]
    fn parent_cycle_does_not_hang_and_bytes_still_count() {
        let entries = vec![
            e(30, 31, "x", 0, true),
            e(31, 30, "y", 0, true),
            e(32, 30, "f", 9, false),
        ];
        let t = Tree::build(PathBuf::from("D:\\"), 5, entries, ScanMethod::Mft, 1, 0);
        assert_eq!(t.summary.bytes, 9);
    }

    #[test]
    fn extensions() {
        assert_eq!(extension("Movie.MP4"), "mp4");
        assert_eq!(extension(".gitignore"), "");
        assert_eq!(extension("README"), "");
    }
}
