//! codon-buffer — a `Buffer` trait that captures the minimal read surface
//! every consumer (git pane, search, agent, editor) needs over a text
//! buffer. The default impl is for Zed's `language::Buffer`; the trait
//! exists so a future Helix-document impl can plug in without rewriting
//! every consumer.
//!
//! ## What's intentionally *not* in the trait
//!
//! Edits — `language::Buffer::edit` is generic over its iterator + offset
//! types and requires `&mut Context<Self>`, neither of which can be
//! expressed in a `dyn`-safe shape. Code paths that mutate a buffer keep
//! using the concrete `Entity<language::Buffer>` for now; the trait
//! covers the read paths (snapshot, language metadata, anchors, version
//! / dirty / file info) that git and search panes actually need.
//!
//! If a future consumer needs to drive edits through the abstraction
//! we'll add a separate non-`dyn`-safe `BufferMut` trait or a workspace
//! verb that owns the entity update.

use std::sync::Arc;

use encoding_rs::Encoding;
use fs::MTime;
use language::{BufferSnapshot, Capability, File, Language};
use text::Anchor;

/// Minimal read surface over a text buffer. See the crate docs for why
/// edits aren't part of the trait.
pub trait Buffer {
    fn snapshot(&self) -> BufferSnapshot;
    fn text_snapshot(&self) -> text::BufferSnapshot;
    fn file(&self) -> Option<&Arc<dyn File>>;
    fn language(&self) -> Option<&Arc<Language>>;
    fn is_dirty(&self) -> bool;
    fn saved_version(&self) -> &clock::Global;
    fn saved_mtime(&self) -> Option<MTime>;
    fn encoding(&self) -> &'static Encoding;
    fn has_bom(&self) -> bool;
    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn anchor_before(&self, offset: usize) -> Anchor;
    fn anchor_after(&self, offset: usize) -> Anchor;
    fn capability(&self) -> Capability;
}

impl Buffer for language::Buffer {
    fn snapshot(&self) -> BufferSnapshot {
        language::Buffer::snapshot(self)
    }
    fn text_snapshot(&self) -> text::BufferSnapshot {
        language::Buffer::text_snapshot(self)
    }
    fn file(&self) -> Option<&Arc<dyn File>> {
        language::Buffer::file(self)
    }
    fn language(&self) -> Option<&Arc<Language>> {
        language::Buffer::language(self)
    }
    fn is_dirty(&self) -> bool {
        language::Buffer::is_dirty(self)
    }
    fn saved_version(&self) -> &clock::Global {
        language::Buffer::saved_version(self)
    }
    fn saved_mtime(&self) -> Option<MTime> {
        language::Buffer::saved_mtime(self)
    }
    fn encoding(&self) -> &'static Encoding {
        language::Buffer::encoding(self)
    }
    fn has_bom(&self) -> bool {
        language::Buffer::has_bom(self)
    }
    fn len(&self) -> usize {
        // `language::Buffer` inherits `len` via Deref → TextBuffer.
        (**self).len()
    }
    fn anchor_before(&self, offset: usize) -> Anchor {
        // Both `anchor_before` and `anchor_after` come from `TextBuffer`
        // via `Deref`. Naming them `language::Buffer::anchor_before` would
        // re-route through the trait method we're implementing (unconditional
        // recursion warning); explicit deref pins the inherent method.
        (**self).anchor_before(offset)
    }
    fn anchor_after(&self, offset: usize) -> Anchor {
        (**self).anchor_after(offset)
    }
    fn capability(&self) -> Capability {
        language::Buffer::capability(self)
    }
}
