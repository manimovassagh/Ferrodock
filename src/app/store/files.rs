//! What the Files tab shows: the open folder of one container, a preview
//! of the selected file, and how many copies are running.

use crate::docker::{FileOp, Listing, Preview, parent_path};

pub struct FileView {
    pub container_id: String,
    pub path: String,
    /// None while loading.
    pub listing: Option<Result<Listing, String>>,
    /// The previewed file, and its contents once loaded.
    pub preview: Option<(String, Option<Result<Preview, String>>)>,
    /// Downloads and uploads in flight.
    pub transfers: usize,
    /// The folder changed (an upload landed) and should be listed again.
    pub stale: bool,
}

impl FileView {
    fn new(container_id: &str, path: &str) -> Self {
        Self {
            container_id: container_id.to_owned(),
            path: path.to_owned(),
            listing: None,
            preview: None,
            transfers: 0,
            stale: false,
        }
    }

    /// Whether `container`'s folder `path` is shown, or being loaded.
    pub fn shows(&self, container: &str, path: &str) -> bool {
        self.container_id == container && self.path == path && !self.stale
    }
}

/// Records that `op` was asked for.
pub fn requested(view: &mut Option<FileView>, op: &FileOp) {
    let (FileOp::List { container, .. }
    | FileOp::Preview { container, .. }
    | FileOp::Download { container, .. }
    | FileOp::Upload { container, .. }) = op
    else {
        return;
    };
    if view.as_ref().is_none_or(|v| v.container_id != *container) {
        *view = Some(FileView::new(container, "/"));
    }
    let Some(v) = view else { return };
    match op {
        FileOp::List { path, .. } => {
            // A preview of a file in another folder no longer applies.
            if v.preview
                .as_ref()
                .is_some_and(|(file, _)| parent_path(file) != *path)
            {
                v.preview = None;
            }
            v.path = path.clone();
            v.listing = None;
            v.stale = false;
        }
        FileOp::Preview { path, .. } => v.preview = Some((path.clone(), None)),
        FileOp::Download { .. } | FileOp::Upload { .. } => v.transfers += 1,
        FileOp::BrowseVolume { .. } | FileOp::EndBrowse { .. } => {}
    }
}

pub fn listed(view: &mut Option<FileView>, result: Result<Listing, (String, String, String)>) {
    let (container, path) = match &result {
        Ok(l) => (l.container_id.clone(), l.path.clone()),
        Err((c, p, _)) => (c.clone(), p.clone()),
    };
    if let Some(v) = view
        && v.container_id == container
        && v.path == path
    {
        v.listing = Some(result.map_err(|(_, _, e)| e));
    }
}

pub fn previewed(
    view: &mut Option<FileView>,
    container: &str,
    path: &str,
    preview: Result<Preview, String>,
) {
    if let Some(v) = view
        && v.container_id == container
        && let Some((asked, slot)) = &mut v.preview
        && asked == path
    {
        *slot = Some(preview);
    }
}

pub fn transferred(view: &mut Option<FileView>, container: &str, changed_dir: Option<&str>) {
    if let Some(v) = view
        && v.container_id == container
    {
        v.transfers = v.transfers.saturating_sub(1);
        if changed_dir == Some(v.path.as_str()) {
            v.stale = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(path: &str) -> FileOp {
        FileOp::List {
            container: "c1".into(),
            path: path.into(),
        }
    }

    #[test]
    fn follows_requests_and_answers() {
        let mut view = None;
        requested(&mut view, &list("/etc"));
        assert!(view.as_ref().unwrap().shows("c1", "/etc"));
        let listing = Listing {
            container_id: "c1".into(),
            path: "/etc".into(),
            entries: Vec::new(),
            complete: true,
        };
        // An answer for a folder no longer open is dropped.
        listed(
            &mut view,
            Ok(Listing {
                path: "/".into(),
                ..listing.clone()
            }),
        );
        assert!(view.as_ref().unwrap().listing.is_none());
        listed(&mut view, Ok(listing));
        assert!(view.as_ref().unwrap().listing.is_some());

        requested(
            &mut view,
            &FileOp::Upload {
                container: "c1".into(),
                dir: "/etc".into(),
                files: Vec::new(),
            },
        );
        assert_eq!(view.as_ref().unwrap().transfers, 1);
        transferred(&mut view, "c1", Some("/etc"));
        let v = view.as_ref().unwrap();
        assert_eq!(v.transfers, 0);
        assert!(!v.shows("c1", "/etc"), "an upload makes the folder stale");

        // Another container starts fresh.
        requested(&mut view, &list("/"));
        requested(
            &mut view,
            &FileOp::List {
                container: "c2".into(),
                path: "/".into(),
            },
        );
        assert_eq!(view.as_ref().unwrap().container_id, "c2");
    }
}
