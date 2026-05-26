use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use gpui::{ImageSource, Resource};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct MarkdownImageDestination {
    pub(super) raw: String,
    pub(super) alt_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum ResolvedMarkdownImage {
    Remote { uri: String },
    Local { path: PathBuf },
    Invalid { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct MarkdownImageSource {
    destination: MarkdownImageDestination,
    resolved: ResolvedMarkdownImage,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct MarkdownImageSourceKey {
    raw: String,
    resolved: ResolvedMarkdownImage,
}

impl MarkdownImageSource {
    pub(super) fn resolve(
        raw: impl Into<String>,
        alt_text: impl Into<String>,
        document_path: Option<&Path>,
    ) -> Self {
        let destination = MarkdownImageDestination {
            raw: raw.into(),
            alt_text: alt_text.into(),
        };
        let resolved = resolve_markdown_image_destination(&destination.raw, document_path);

        Self {
            destination,
            resolved,
        }
    }

    pub(super) fn resource(&self) -> Option<Resource> {
        match &self.resolved {
            ResolvedMarkdownImage::Remote { uri } => Some(Resource::Uri(uri.clone().into())),
            ResolvedMarkdownImage::Local { path } => {
                Some(Resource::Path(Arc::from(path.as_path())))
            }
            ResolvedMarkdownImage::Invalid { .. } => None,
        }
    }

    pub(super) fn image_source(&self) -> Option<ImageSource> {
        self.resource().map(ImageSource::Resource)
    }

    pub(super) fn cache_key(&self) -> MarkdownImageSourceKey {
        MarkdownImageSourceKey {
            raw: self.destination.raw.clone(),
            resolved: self.resolved.clone(),
        }
    }

    #[cfg(test)]
    pub(super) fn raw_destination(&self) -> &str {
        &self.destination.raw
    }

    pub(super) fn is_renderable(&self) -> bool {
        self.resource().is_some()
    }

    pub(super) fn fallback_label(&self) -> String {
        if !self.destination.alt_text.trim().is_empty() {
            return self.destination.alt_text.clone();
        }

        self.destination
            .raw
            .split('?')
            .next()
            .unwrap_or(&self.destination.raw)
            .to_string()
    }
}

fn resolve_markdown_image_destination(
    raw_destination: &str,
    document_path: Option<&Path>,
) -> ResolvedMarkdownImage {
    if raw_destination.starts_with("http://") || raw_destination.starts_with("https://") {
        return ResolvedMarkdownImage::Remote {
            uri: raw_destination.to_string(),
        };
    }

    if let Some(path) = raw_destination.strip_prefix("file://") {
        if path.is_empty() {
            return ResolvedMarkdownImage::Invalid {
                reason: "empty file URI path".to_string(),
            };
        }
        return ResolvedMarkdownImage::Local {
            path: PathBuf::from(path),
        };
    }

    let path = PathBuf::from(raw_destination);
    if path.is_absolute() {
        return ResolvedMarkdownImage::Local { path };
    }

    if looks_like_unsupported_uri_scheme(raw_destination) {
        return ResolvedMarkdownImage::Invalid {
            reason: "unsupported URI scheme".to_string(),
        };
    }

    if let Some(parent) = document_path.and_then(Path::parent) {
        return ResolvedMarkdownImage::Local {
            path: parent.join(path),
        };
    }

    ResolvedMarkdownImage::Invalid {
        reason: "relative image path has no document path".to_string(),
    }
}

fn looks_like_unsupported_uri_scheme(raw_destination: &str) -> bool {
    let Some(colon_index) = raw_destination.find(':') else {
        return false;
    };

    #[cfg(windows)]
    {
        if colon_index == 1
            && raw_destination
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic)
        {
            return false;
        }
    }

    let first_separator = raw_destination
        .find(['/', '\\'])
        .unwrap_or(raw_destination.len());
    colon_index < first_separator
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_http_and_https_as_remote() {
        assert!(matches!(
            MarkdownImageSource::resolve("https://example.com/cat.png", "", None)
                .resource()
                .unwrap(),
            Resource::Uri(_)
        ));
        assert!(matches!(
            MarkdownImageSource::resolve("http://example.com/cat.png", "", None)
                .resource()
                .unwrap(),
            Resource::Uri(_)
        ));
    }

    #[test]
    fn resolves_absolute_local_paths() {
        let path = std::env::current_dir().unwrap().join("cat.png");
        let source = MarkdownImageSource::resolve(path.to_string_lossy().to_string(), "", None);

        assert!(matches!(source.resource().unwrap(), Resource::Path(_)));
    }

    #[test]
    fn resolves_file_uri_paths() {
        let source = MarkdownImageSource::resolve("file:///tmp/cat.png", "", None);

        assert!(matches!(source.resource().unwrap(), Resource::Path(_)));
    }

    #[test]
    fn resolves_relative_paths_against_document_parent() {
        let document_path = Path::new("/tmp/docs/readme.md");
        let source = MarkdownImageSource::resolve("./cat.png", "", Some(document_path));

        assert_eq!(
            source.resource(),
            Some(Resource::Path(Arc::from(Path::new("/tmp/docs/./cat.png"))))
        );
    }

    #[test]
    fn rejects_relative_paths_without_document_path() {
        let source = MarkdownImageSource::resolve("./cat.png", "", None);

        assert!(source.resource().is_none());
    }

    #[test]
    fn rejects_unsupported_uri_schemes() {
        let source = MarkdownImageSource::resolve("ftp://example.com/cat.png", "", None);

        assert!(source.resource().is_none());
    }

    #[cfg(windows)]
    #[test]
    fn windows_drive_paths_are_not_rejected_as_uri_schemes() {
        let source = MarkdownImageSource::resolve("C:\\images\\cat.png", "", None);

        assert!(matches!(source.resource().unwrap(), Resource::Path(_)));
    }
}
