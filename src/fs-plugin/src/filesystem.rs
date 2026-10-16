#[derive(Clone, Copy, Debug)]
pub struct Entry {
    pub name: &'static str,
    pub is_directory: bool,
    pub contents: &'static [u8],
}

const ROOT_ENTRIES: [Entry; 3] = [
    Entry {
        name: "hello.txt",
        is_directory: false,
        contents: b"Hello from the Rust WFX plugin.\n",
    },
    Entry {
        name: "readme.txt",
        is_directory: false,
        contents: b"This is a small virtual filesystem exposed by TotalDroid.\n",
    },
    Entry {
        name: "docs",
        is_directory: true,
        contents: b"",
    },
];

const DOCS_ENTRIES: [Entry; 1] = [Entry {
    name: "guide.txt",
    is_directory: false,
    contents: b"Double Commander can browse this directory through the WFX interface.\n",
}];

pub fn list_directory(path: &str) -> Option<&'static [Entry]> {
    match normalize_path(path).as_str() {
        "" => Some(&ROOT_ENTRIES),
        "docs" => Some(&DOCS_ENTRIES),
        _ => None,
    }
}

pub fn get_file(path: &str) -> Option<&'static [u8]> {
    match normalize_path(path).as_str() {
        "hello.txt" => Some(ROOT_ENTRIES[0].contents),
        "readme.txt" => Some(ROOT_ENTRIES[1].contents),
        "docs/guide.txt" => Some(DOCS_ENTRIES[0].contents),
        _ => None,
    }
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim_matches('/')
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::{get_file, list_directory};

    #[test]
    fn lists_root_and_nested_directory() {
        assert_eq!(list_directory("\\").unwrap().len(), 3);
        assert_eq!(list_directory("\\docs\\").unwrap()[0].name, "guide.txt");
        assert!(list_directory("missing").is_none());
    }

    #[test]
    fn retrieves_virtual_files() {
        assert_eq!(
            get_file("\\docs\\guide.txt").unwrap(),
            b"Double Commander can browse this directory through the WFX interface.\n"
        );
        assert!(get_file("docs").is_none());
        assert!(get_file("../hello.txt").is_none());
    }
}
