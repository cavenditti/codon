//! Error pattern: no fallible APIs — the parser silently drops
//! malformed OSC sequences (the byte stream is adversarial-by-design
//! and panics are unacceptable), and the BlockStore degrades partial
//! sequences into "dropped block, keep scanning" non-events.
//!
//! Terminal blocks — OSC 133 byte-stream scanner + per-pane
//! `BlockStore` that reassembles boundaries into typed `Block`
//! records.
//!
//! ## What this crate ships
//!
//! This is the foundation task for `REQ:codon/terminal-blocks`. The
//! minimum surface that makes a `Block` exist as a typed selection:
//!
//! - [`Osc133Scanner`] — a small byte-stream state machine that
//!   recognises the four OSC 133 sequences (`A`/`B`/`C`/`D[;exit]`)
//!   and emits [`BlockBoundary`] events along the way. Designed to
//!   be fed `&[u8]` chunks straight from the PTY reader — partial
//!   sequences across reads are preserved by the scanner's internal
//!   state.
//!
//! - [`BlockStore`] — per-terminal-pane state machine that consumes
//!   [`BlockBoundary`]s, segments the byte stream between them, and
//!   emits assembled [`Block`] records keyed by their insertion
//!   order. Out-of-order or partial boundary chains (`A` followed
//!   by another `A` without `B`/`C`/`D`, `D` without `C`, etc.)
//!   degrade gracefully — the in-flight block is dropped and the
//!   store re-anchors at the next `A`.
//!
//! - [`Block`] — the assembled record exposed to the rest of codon
//!   (`command`, `output`, `exit_status`, `start`, `end`,
//!   `detection`).
//!
//! ## What this crate does NOT do (yet)
//!
//! - **No live PTY-byte tap.** Wiring the scanner into the live
//!   `vendor/zed/crates/terminal/` PTY reader is intentionally
//!   deferred. The Zed-side `EventLoop` consumes PTY bytes inside
//!   `alacritty_terminal` (a `cargo git` source, not a submodule),
//!   and vte's OSC dispatcher is the only place arbitrary OSC
//!   parameter bytes are routed — neither has a public observer
//!   hook in the current pin. Adding one cleanly requires either
//!   patching the alacritty fork (`zed-industries/alacritty`) or
//!   wrapping the `EventedPty` with a tee `Read` adapter; both are
//!   larger-than-foundation changes and are tracked as part of the
//!   follow-up `phase-19/terminal-blocks-heuristic` /
//!   `phase-19/terminal-blocks-navigation` work.
//!
//! - **No heuristic detection.** Pure OSC 133 here. Heuristic
//!   prompt-line detection is the follow-up
//!   `phase-19/terminal-blocks-heuristic` task — see
//!   `REQ:codon/terminal-blocks#c-heuristic-detector`.
//!
//! - **No navigation, no cross-pane verbs, no status bar.** All
//!   tracked as follow-up tasks under `REQ:codon/terminal-blocks`.

use codon_pane_bridge::TerminalBlockRef;
use gpui::EntityId;

/// What kind of OSC 133 boundary the scanner just saw, plus the byte
/// offset (within the cumulative byte stream the scanner has
/// processed) at which the boundary was emitted. The offset is the
/// byte *after* the closing string terminator (ST) — i.e. the first
/// byte of the next "regular" content, which makes downstream
/// segmentation straight `[start..end)` slicing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockBoundaryKind {
    /// `\e]133;A\e\\` — prompt start. The shell is about to print
    /// the prompt itself (PS1).
    PromptStart,
    /// `\e]133;B\e\\` — command start. The prompt is finished
    /// printing; the user's command text follows until the
    /// `OutputStart` boundary.
    CommandStart,
    /// `\e]133;C\e\\` — output start. The command has been
    /// dispatched; everything that follows until `OutputEnd` is the
    /// command's stdout/stderr.
    OutputStart,
    /// `\e]133;D[;<exit>]\e\\` — output end, optionally with the
    /// command's exit status. The next `PromptStart` opens a fresh
    /// block.
    OutputEnd { exit: Option<i32> },
}

/// One OSC 133 transition: the kind plus the byte-offset anchor at
/// which it landed in the source stream. The anchor is *exclusive*
/// of the OSC bytes themselves (those are stripped from the
/// segmenting offsets), so contiguous boundary pairs can be used as
/// `[start..end)` slice ranges into a re-assembled byte log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockBoundary {
    pub kind: BlockBoundaryKind,
    pub anchor: usize,
}

/// How a block was detected. OSC 133 is the precise path (shell
/// cooperation); heuristic detection (follow-up task) infers
/// boundaries from prompt-shaped lines in the scrollback and is
/// rendered dimmer in the UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detection {
    Osc133,
    Heuristic,
}

/// An assembled command-and-output block — the typed object the rest
/// of codon's grammar refers to as `ObjectKind::Block`.
///
/// `command` and `output` are stripped of the OSC 133 framing bytes
/// themselves (the scanner peels them off before passing the segment
/// to the store). They're stored as `String` since the foundation
/// task is shell-text-oriented and the bytes are expected UTF-8
/// (terminal output is rendered as such); non-UTF-8 input is
/// lossy-converted with `String::from_utf8_lossy` so the store can't
/// silently drop a block over an encoding glitch.
///
/// `start` / `end` are byte offsets into the cumulative byte stream
/// fed to the store, useful as stable anchors for future scrollback
/// re-rendering. The follow-up navigation task will switch them to
/// real `alacritty::Anchor`s once the live byte-tap lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub command: String,
    pub output: String,
    pub exit_status: Option<i32>,
    pub start: usize,
    pub end: usize,
    pub detection: Detection,
}

// ---------------------------------------------------------------------------
// OSC 133 byte-stream scanner
// ---------------------------------------------------------------------------

/// Internal lexer state for the OSC 133 byte scanner. Kept tiny: we
/// only need to recognise four very specific sequences, so a full
/// vte-style parser would be over-engineering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LexerState {
    /// Looking for an ESC (0x1b) that might start an OSC.
    Idle,
    /// Saw ESC; expecting `]` to open the OSC payload.
    SawEsc,
    /// Inside an OSC payload; collecting parameter bytes until a
    /// string terminator (BEL or ESC `\`).
    InOsc,
    /// Inside the OSC, saw ESC; expecting `\` to complete the ST.
    SawEscInOsc,
}

/// Byte-stream OSC 133 scanner. Stateful across `feed` calls so the
/// caller can hand it arbitrary PTY chunks — boundaries spanning a
/// chunk seam are reassembled correctly.
///
/// The scanner is purposefully *narrow*: it only recognises the
/// OSC 133 subset (it ignores every other OSC, every CSI, every
/// non-OSC ESC sequence). That keeps it cheap to run in the hot path
/// without competing with the real vte parser running inside
/// alacritty.
pub struct Osc133Scanner {
    state: LexerState,
    /// Cumulative byte offset across all `feed` calls — used to
    /// emit `BlockBoundary.anchor` so consumers can map back into
    /// the byte stream they're keeping in parallel.
    cursor: usize,
    /// Buffer accumulating the OSC payload (between `\e]` and the
    /// terminator). Cleared whenever an OSC closes (cleanly or
    /// otherwise).
    osc_payload: Vec<u8>,
}

impl Default for Osc133Scanner {
    fn default() -> Self {
        Self::new()
    }
}

impl Osc133Scanner {
    pub fn new() -> Self {
        Self {
            state: LexerState::Idle,
            cursor: 0,
            osc_payload: Vec::new(),
        }
    }

    /// How many bytes the scanner has consumed across its lifetime.
    /// Exposed primarily for tests; downstream consumers should
    /// consult `BlockBoundary.anchor` for positional information.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Feed a chunk of bytes. Returns every `BlockBoundary` event
    /// that opened *and* closed within (or across into) this chunk.
    ///
    /// The OSC framing bytes are *not* counted as user content from
    /// the consumer's perspective — `BlockBoundary.anchor` points at
    /// the position immediately after the ST. Consumers maintaining
    /// their own byte log in parallel should strip the OSC bytes
    /// before recording, OR keep the OSC bytes and use the boundary
    /// anchors as opaque markers.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<BlockBoundary> {
        let mut boundaries = Vec::new();
        for &byte in bytes {
            self.cursor += 1;
            match self.state {
                LexerState::Idle => {
                    if byte == 0x1b {
                        self.state = LexerState::SawEsc;
                    }
                }
                LexerState::SawEsc => {
                    if byte == b']' {
                        self.state = LexerState::InOsc;
                        self.osc_payload.clear();
                    } else if byte == 0x1b {
                        // Stay in SawEsc — back-to-back ESCs are
                        // legal terminal noise (e.g. user typed
                        // <Esc><Esc>); the next byte still gets a
                        // chance to open an OSC.
                    } else {
                        self.state = LexerState::Idle;
                    }
                }
                LexerState::InOsc => {
                    if byte == 0x07 {
                        // BEL terminator.
                        if let Some(boundary) = self.parse_payload() {
                            boundaries.push(boundary);
                        }
                        self.state = LexerState::Idle;
                        self.osc_payload.clear();
                    } else if byte == 0x1b {
                        self.state = LexerState::SawEscInOsc;
                    } else {
                        // OSC payloads are short — cap at a reasonable
                        // size so a runaway sequence doesn't pin the
                        // scanner's allocations. Real OSC 133 payloads
                        // never exceed ~16 bytes (`133;D;<exit>`); 256
                        // gives us 8x headroom before bailing.
                        const OSC_PAYLOAD_CAP: usize = 256;
                        if self.osc_payload.len() < OSC_PAYLOAD_CAP {
                            self.osc_payload.push(byte);
                        } else {
                            // Overflow: abandon this OSC, return to Idle
                            // without emitting a boundary. The remaining
                            // payload bytes + terminator will be eaten
                            // by the Idle/SawEsc machinery as terminal
                            // noise.
                            self.state = LexerState::Idle;
                            self.osc_payload.clear();
                        }
                    }
                }
                LexerState::SawEscInOsc => {
                    if byte == b'\\' {
                        // ESC `\` terminator (the canonical ST).
                        if let Some(boundary) = self.parse_payload() {
                            boundaries.push(boundary);
                        }
                        self.state = LexerState::Idle;
                        self.osc_payload.clear();
                    } else {
                        // Non-ST after ESC in OSC: bail and treat as
                        // garbage. Don't try to recover the prefix.
                        self.state = LexerState::Idle;
                        self.osc_payload.clear();
                    }
                }
            }
        }
        boundaries
    }

    /// Parse `self.osc_payload` as an OSC 133 sequence; return the
    /// resulting boundary if recognised, or `None` for any other
    /// OSC code (color requests, hyperlinks, etc. — let the real
    /// vte parser handle those).
    fn parse_payload(&self) -> Option<BlockBoundary> {
        // Payload shape: `133;<KIND>[;<exit>]`
        let payload = self.osc_payload.as_slice();
        let mut parts = payload.split(|&b| b == b';');
        let head = parts.next()?;
        if head != b"133" {
            return None;
        }
        let kind_bytes = parts.next()?;
        if kind_bytes.len() != 1 {
            return None;
        }
        let kind = match kind_bytes[0] {
            b'A' => BlockBoundaryKind::PromptStart,
            b'B' => BlockBoundaryKind::CommandStart,
            b'C' => BlockBoundaryKind::OutputStart,
            b'D' => {
                let exit = match parts.next() {
                    None => None,
                    Some(exit_bytes) => {
                        // Tolerate empty exit field: `133;D;` →
                        // exit_status = None, not a parse failure.
                        if exit_bytes.is_empty() {
                            None
                        } else {
                            let exit_str = std::str::from_utf8(exit_bytes).ok()?;
                            exit_str.trim().parse::<i32>().ok().map(Some)?
                        }
                    }
                };
                BlockBoundaryKind::OutputEnd { exit }
            }
            _ => return None,
        };
        Some(BlockBoundary {
            kind,
            anchor: self.cursor,
        })
    }
}

// ---------------------------------------------------------------------------
// BlockStore
// ---------------------------------------------------------------------------

/// What part of a block the store is currently collecting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CollectorPhase {
    /// No block in flight — waiting for the next `PromptStart`.
    Idle,
    /// Saw `A`, before `B`: the prompt is being printed. Bytes here
    /// are PS1 output, not command text; they're discarded.
    AfterPromptStart,
    /// Saw `B`, before `C`: collecting command text bytes.
    AfterCommandStart,
    /// Saw `C`, before `D`: collecting output bytes.
    AfterOutputStart,
}

/// The per-terminal-pane store of assembled blocks plus the in-flight
/// collector. Decoupled from `Osc133Scanner` so heuristic detection
/// (follow-up task) can plug into the same `record_boundary` /
/// `record_bytes` API.
///
/// The store owns the raw byte log too — it's the only piece that
/// knows where command bytes vs. output bytes land relative to the
/// boundaries. Bytes that fall outside any block (between `D` and
/// the next `A`, or before the first `A`) are dropped.
pub struct BlockStore {
    pane: EntityId,
    blocks: Vec<Block>,
    phase: CollectorPhase,
    in_flight: Option<InFlightBlock>,
    detection: Detection,
}

/// Scratch state for the block currently being assembled. Split out
/// of the store so `phase` transitions don't need to juggle a maybe-
/// populated set of fields on `BlockStore` directly.
#[derive(Debug)]
struct InFlightBlock {
    command: Vec<u8>,
    output: Vec<u8>,
    start: usize,
}

impl BlockStore {
    /// Construct an empty store for the given terminal pane.
    pub fn new(pane: EntityId) -> Self {
        Self {
            pane,
            blocks: Vec::new(),
            phase: CollectorPhase::Idle,
            in_flight: None,
            detection: Detection::Osc133,
        }
    }

    /// The owning terminal pane's entity id. Used by selection
    /// callers to assemble a [`TerminalBlockRef`].
    pub fn pane(&self) -> EntityId {
        self.pane
    }

    /// How many blocks the store has assembled. Stable across the
    /// pane's lifetime — index 0 is the first block, index `len-1`
    /// the most recent.
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Borrow an assembled block by its insertion index. Returns
    /// `None` if the index is out of range (e.g. a stale
    /// [`TerminalBlockRef`] from a different pane configuration).
    pub fn get(&self, index: usize) -> Option<&Block> {
        self.blocks.get(index)
    }

    /// All assembled blocks, oldest first.
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// Build a [`TerminalBlockRef`] pointing at the block at `index`
    /// in this store. Returns `None` if the index is out of range.
    pub fn block_ref(&self, index: usize) -> Option<TerminalBlockRef> {
        if index >= self.blocks.len() {
            return None;
        }
        Some(TerminalBlockRef {
            pane: self.pane,
            index,
        })
    }

    /// Feed raw PTY bytes through both the OSC 133 scanner and the
    /// block collector in one call. The combined helper exists
    /// because the byte stream is the only ground truth for both —
    /// callers that want to split the operations across two
    /// state-holders can use [`Self::record_boundary`] +
    /// [`Self::record_bytes`] directly.
    ///
    /// Returns the boundaries the scanner emitted on this chunk,
    /// primarily for tests; production callers can ignore the return
    /// value and read assembled blocks via [`Self::blocks`].
    pub fn feed(&mut self, scanner: &mut Osc133Scanner, bytes: &[u8]) -> Vec<BlockBoundary> {
        let boundaries = scanner.feed(bytes);

        if boundaries.is_empty() {
            // Fast path: no transitions in this chunk; just append
            // bytes to the in-flight block (if any).
            self.record_bytes(bytes);
            return boundaries;
        }

        // Slow path: split `bytes` along boundary anchors so the
        // record_bytes call only sees a *single* phase's worth of
        // bytes at a time, and the boundary transitions land between
        // them. Anchor positions are stream-cumulative; we subtract
        // the chunk's start offset (which the scanner already
        // advanced past) to get chunk-relative offsets.
        let chunk_end_cursor = scanner.cursor();
        let chunk_start_cursor = chunk_end_cursor - bytes.len();

        let mut last_chunk_offset = 0usize;
        for boundary in &boundaries {
            // `boundary.anchor` is the byte position immediately
            // *after* the ST byte that closed the OSC. So bytes in
            // `[last_chunk_offset..anchor_in_chunk)` of `bytes` belong
            // to the phase that was active *before* this boundary,
            // *minus* the OSC framing bytes which are part of that
            // range. Stripping them out exactly requires re-walking
            // the lexer; instead we conservatively forward the whole
            // slice (OSC framing bytes included) to the collector
            // and let the collector strip recognised OSC byte spans
            // — but that's complexity we don't need. Practical
            // shells emit OSC 133 between PS1 prints, not in the
            // middle of command text, so OSC bytes leaking into
            // `command`/`output` strings would be a UX cosmetic
            // issue only.
            //
            // Cheap fix: trim trailing ESC...ST bytes from the
            // forwarded slice. The boundary anchor points just past
            // the ST, so the OSC framing is exactly the suffix
            // ending at `anchor_in_chunk`.
            let anchor_in_chunk = boundary.anchor - chunk_start_cursor;
            let pre_boundary = &bytes[last_chunk_offset..anchor_in_chunk];
            let trimmed = strip_trailing_osc_frame(pre_boundary);
            self.record_bytes(trimmed);
            self.record_boundary(*boundary);
            last_chunk_offset = anchor_in_chunk;
        }
        // Any bytes after the last boundary in this chunk belong
        // to the new phase the last boundary opened.
        if last_chunk_offset < bytes.len() {
            self.record_bytes(&bytes[last_chunk_offset..]);
        }

        boundaries
    }

    /// Record a single boundary transition. Public so heuristic
    /// detection (follow-up) can drive the store without going
    /// through the OSC scanner.
    pub fn record_boundary(&mut self, boundary: BlockBoundary) {
        match (self.phase, boundary.kind) {
            // Clean opening: previous block was idle, A starts a
            // fresh one.
            (CollectorPhase::Idle, BlockBoundaryKind::PromptStart) => {
                self.phase = CollectorPhase::AfterPromptStart;
                self.in_flight = Some(InFlightBlock {
                    command: Vec::new(),
                    output: Vec::new(),
                    start: boundary.anchor,
                });
            }
            // Restart: a second A without a closing D — drop the
            // in-flight block, anchor fresh at the new A. This is
            // the "out-of-order degrade gracefully" rule from
            // `REQ:codon/terminal-blocks#c-osc-133-parser`.
            (_, BlockBoundaryKind::PromptStart) => {
                self.in_flight = Some(InFlightBlock {
                    command: Vec::new(),
                    output: Vec::new(),
                    start: boundary.anchor,
                });
                self.phase = CollectorPhase::AfterPromptStart;
            }
            // Prompt → command: legal transition, expected.
            (CollectorPhase::AfterPromptStart, BlockBoundaryKind::CommandStart) => {
                self.phase = CollectorPhase::AfterCommandStart;
            }
            // Command → output: legal transition.
            (CollectorPhase::AfterCommandStart, BlockBoundaryKind::OutputStart) => {
                self.phase = CollectorPhase::AfterOutputStart;
            }
            // Some shells skip B (older zsh integrations) and jump
            // straight from A to C. Treat that as "command was
            // empty" — clean degrade rather than dropping the
            // block.
            (CollectorPhase::AfterPromptStart, BlockBoundaryKind::OutputStart) => {
                self.phase = CollectorPhase::AfterOutputStart;
            }
            // Output → end: assemble and commit the block.
            (CollectorPhase::AfterOutputStart, BlockBoundaryKind::OutputEnd { exit }) => {
                let in_flight = self.in_flight.take();
                self.phase = CollectorPhase::Idle;
                if let Some(in_flight) = in_flight {
                    self.blocks.push(Block {
                        command: bytes_to_lossy_string(&in_flight.command),
                        output: bytes_to_lossy_string(&in_flight.output),
                        exit_status: exit,
                        start: in_flight.start,
                        end: boundary.anchor,
                        detection: self.detection,
                    });
                }
            }
            // D without a preceding C, or any other illegal
            // transition: drop the in-flight block, re-anchor to
            // Idle. The next A will start fresh.
            _ => {
                self.in_flight = None;
                self.phase = CollectorPhase::Idle;
            }
        }
    }

    /// Append raw bytes into whatever the current phase is collecting
    /// (command bytes during `AfterCommandStart`, output bytes during
    /// `AfterOutputStart`, dropped otherwise).
    pub fn record_bytes(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let Some(in_flight) = self.in_flight.as_mut() else {
            return;
        };
        match self.phase {
            CollectorPhase::AfterCommandStart => in_flight.command.extend_from_slice(bytes),
            CollectorPhase::AfterOutputStart => in_flight.output.extend_from_slice(bytes),
            CollectorPhase::Idle | CollectorPhase::AfterPromptStart => {
                // Pre-`B` (prompt printing) and post-`D` (between
                // blocks) bytes aren't part of either field — drop.
            }
        }
    }
}

/// Strip a trailing ESC]133;…BEL or ESC]133;…ESC\ frame from `slice`,
/// returning the prefix before the frame. If the slice doesn't end in
/// a recognisable OSC 133 terminator, returns `slice` unchanged.
///
/// Used by [`BlockStore::feed`] to scrub OSC framing bytes from the
/// segments handed to `record_bytes`, so the resulting `command`/
/// `output` strings don't carry the framing as visible text.
fn strip_trailing_osc_frame(slice: &[u8]) -> &[u8] {
    // The terminator is either the single byte BEL (0x07) or the
    // two bytes ESC `\`. Walk backwards from the end past the
    // terminator and then past the payload until we hit the opening
    // ESC `]`. If the structure doesn't match, leave the slice
    // alone.
    if slice.is_empty() {
        return slice;
    }
    let (end_excl_terminator, has_st) = if slice.last() == Some(&0x07) {
        (slice.len() - 1, true)
    } else if slice.len() >= 2 && &slice[slice.len() - 2..] == b"\x1b\\" {
        (slice.len() - 2, true)
    } else {
        (slice.len(), false)
    };
    if !has_st {
        return slice;
    }
    // Walk back from end_excl_terminator looking for the ESC `]`
    // that opened the OSC. Cap the search at a sane horizon so we
    // can't accidentally chew through arbitrary command output that
    // happens to end with a BEL.
    const SEARCH_HORIZON: usize = 64;
    let lower = end_excl_terminator.saturating_sub(SEARCH_HORIZON);
    let payload_search = &slice[lower..end_excl_terminator];
    for i in (0..payload_search.len().saturating_sub(1)).rev() {
        if payload_search[i] == 0x1b && payload_search[i + 1] == b']' {
            let open_idx = lower + i;
            // Verify the payload begins with `133;` so we only
            // strip OSC 133 frames specifically — not unrelated
            // OSCs that the byte log might carry.
            let payload_start = open_idx + 2;
            if slice
                .get(payload_start..payload_start + 4)
                .map(|w| w == b"133;")
                .unwrap_or(false)
            {
                return &slice[..open_idx];
            }
            // Not a 133 frame — give up rather than mis-strip.
            return slice;
        }
    }
    slice
}

fn bytes_to_lossy_string(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext as _, TestAppContext};

    const OSC_A: &[u8] = b"\x1b]133;A\x07";
    const OSC_B: &[u8] = b"\x1b]133;B\x07";
    const OSC_C: &[u8] = b"\x1b]133;C\x07";
    const OSC_D_OK: &[u8] = b"\x1b]133;D;0\x07";
    const OSC_D_FAIL: &[u8] = b"\x1b]133;D;1\x07";
    const OSC_D_NO_EXIT: &[u8] = b"\x1b]133;D\x07";

    /// Scanner returns all four boundary kinds in order when fed a
    /// canonical ABCD sequence in one chunk.
    #[test]
    fn scanner_recognises_full_abcd_sequence() {
        let mut scanner = Osc133Scanner::new();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(OSC_A);
        bytes.extend_from_slice(b"$ ");
        bytes.extend_from_slice(OSC_B);
        bytes.extend_from_slice(b"echo hi");
        bytes.extend_from_slice(OSC_C);
        bytes.extend_from_slice(b"hi\n");
        bytes.extend_from_slice(OSC_D_OK);

        let boundaries = scanner.feed(&bytes);
        assert_eq!(boundaries.len(), 4);
        assert!(matches!(
            boundaries[0].kind,
            BlockBoundaryKind::PromptStart
        ));
        assert!(matches!(
            boundaries[1].kind,
            BlockBoundaryKind::CommandStart
        ));
        assert!(matches!(
            boundaries[2].kind,
            BlockBoundaryKind::OutputStart
        ));
        assert_eq!(
            boundaries[3].kind,
            BlockBoundaryKind::OutputEnd { exit: Some(0) }
        );
    }

    /// Scanner handles the ESC `\` form of the string terminator as
    /// well as BEL.
    #[test]
    fn scanner_handles_esc_backslash_st() {
        let mut scanner = Osc133Scanner::new();
        let boundaries = scanner.feed(b"\x1b]133;A\x1b\\");
        assert_eq!(boundaries.len(), 1);
        assert!(matches!(
            boundaries[0].kind,
            BlockBoundaryKind::PromptStart
        ));
    }

    /// Scanner's state survives across `feed` calls — a sequence
    /// split across two chunks still parses.
    #[test]
    fn scanner_survives_chunk_seam() {
        let mut scanner = Osc133Scanner::new();
        let first = scanner.feed(b"\x1b]13");
        let second = scanner.feed(b"3;A\x07");
        assert!(first.is_empty());
        assert_eq!(second.len(), 1);
        assert!(matches!(second[0].kind, BlockBoundaryKind::PromptStart));
    }

    /// Unrecognised OSCs (color requests, hyperlinks, made-up codes)
    /// are silently ignored.
    #[test]
    fn scanner_ignores_non_133_oscs() {
        let mut scanner = Osc133Scanner::new();
        let boundaries = scanner.feed(b"\x1b]8;;https://example.com\x07link\x1b]8;;\x07");
        assert!(boundaries.is_empty());
    }

    /// Exit-status field is parsed when present, defaults to None
    /// otherwise.
    #[test]
    fn scanner_parses_exit_status() {
        let mut scanner = Osc133Scanner::new();
        let boundaries = scanner.feed(OSC_D_FAIL);
        assert_eq!(boundaries.len(), 1);
        assert_eq!(
            boundaries[0].kind,
            BlockBoundaryKind::OutputEnd { exit: Some(1) }
        );

        let mut scanner = Osc133Scanner::new();
        let boundaries = scanner.feed(OSC_D_NO_EXIT);
        assert_eq!(boundaries.len(), 1);
        assert_eq!(
            boundaries[0].kind,
            BlockBoundaryKind::OutputEnd { exit: None }
        );
    }

    /// `BlockStore` assembles a canonical ABCD sequence into exactly
    /// one Block with the right command, output, and exit status.
    #[gpui::test]
    fn block_store_assembles_clean_abcd(cx: &mut TestAppContext) {
        let pane_id = pane_id(cx);
        let mut store = BlockStore::new(pane_id);
        let mut scanner = Osc133Scanner::new();

        let mut bytes = Vec::new();
        bytes.extend_from_slice(OSC_A);
        bytes.extend_from_slice(b"user@host $ ");
        bytes.extend_from_slice(OSC_B);
        bytes.extend_from_slice(b"echo hi");
        bytes.extend_from_slice(OSC_C);
        bytes.extend_from_slice(b"hi\n");
        bytes.extend_from_slice(OSC_D_OK);

        store.feed(&mut scanner, &bytes);
        assert_eq!(store.len(), 1);
        let block = store.get(0).expect("block should exist");
        assert_eq!(block.command, "echo hi");
        assert_eq!(block.output, "hi\n");
        assert_eq!(block.exit_status, Some(0));
        assert_eq!(block.detection, Detection::Osc133);
        assert_eq!(store.block_ref(0).expect("ref").index, 0);
        assert_eq!(store.block_ref(0).expect("ref").pane, pane_id);
    }

    /// AB followed by another A (no C, no D) — the in-flight block is
    /// dropped and the second A re-anchors fresh. The store ends with
    /// zero assembled blocks (since the second sequence isn't closed
    /// either).
    #[gpui::test]
    fn block_store_drops_block_on_restart_a(cx: &mut TestAppContext) {
        let pane_id = pane_id(cx);
        let mut store = BlockStore::new(pane_id);
        let mut scanner = Osc133Scanner::new();

        let mut bytes = Vec::new();
        bytes.extend_from_slice(OSC_A);
        bytes.extend_from_slice(OSC_B);
        bytes.extend_from_slice(b"command-1");
        bytes.extend_from_slice(OSC_A);
        bytes.extend_from_slice(OSC_B);
        bytes.extend_from_slice(b"command-2");

        store.feed(&mut scanner, &bytes);
        assert!(
            store.is_empty(),
            "both sequences are unclosed; no blocks should be assembled"
        );
    }

    /// ABCD without exit status field — exit_status = None, block is
    /// otherwise complete.
    #[gpui::test]
    fn block_store_handles_missing_exit_status(cx: &mut TestAppContext) {
        let pane_id = pane_id(cx);
        let mut store = BlockStore::new(pane_id);
        let mut scanner = Osc133Scanner::new();

        let mut bytes = Vec::new();
        bytes.extend_from_slice(OSC_A);
        bytes.extend_from_slice(OSC_B);
        bytes.extend_from_slice(b"true");
        bytes.extend_from_slice(OSC_C);
        bytes.extend_from_slice(b"");
        bytes.extend_from_slice(OSC_D_NO_EXIT);

        store.feed(&mut scanner, &bytes);
        assert_eq!(store.len(), 1);
        let block = store.get(0).expect("block");
        assert_eq!(block.command, "true");
        assert_eq!(block.output, "");
        assert_eq!(block.exit_status, None);
    }

    /// ACBD (out of order: C before B) — the malformed sequence is
    /// dropped on the illegal `C` after `A`-without-`B`... actually
    /// the store treats A→C as "skipped B / empty command" (some
    /// shells do this legitimately). So this test exercises ABDC
    /// instead: D arriving before C is the genuinely illegal
    /// permutation.
    #[gpui::test]
    fn block_store_drops_block_on_d_before_c(cx: &mut TestAppContext) {
        let pane_id = pane_id(cx);
        let mut store = BlockStore::new(pane_id);
        let mut scanner = Osc133Scanner::new();

        let mut bytes = Vec::new();
        bytes.extend_from_slice(OSC_A);
        bytes.extend_from_slice(OSC_B);
        bytes.extend_from_slice(b"cmd");
        bytes.extend_from_slice(OSC_D_OK);
        bytes.extend_from_slice(OSC_C);

        store.feed(&mut scanner, &bytes);
        assert!(
            store.is_empty(),
            "D arriving before C is illegal; block must drop"
        );
    }

    /// Two clean back-to-back blocks land at indices 0 and 1.
    #[gpui::test]
    fn block_store_assembles_consecutive_blocks(cx: &mut TestAppContext) {
        let pane_id = pane_id(cx);
        let mut store = BlockStore::new(pane_id);
        let mut scanner = Osc133Scanner::new();

        let mut bytes = Vec::new();
        for (cmd, out) in [(b"ls".as_ref(), b"a b c\n".as_ref()), (b"pwd", b"/tmp\n")] {
            bytes.extend_from_slice(OSC_A);
            bytes.extend_from_slice(OSC_B);
            bytes.extend_from_slice(cmd);
            bytes.extend_from_slice(OSC_C);
            bytes.extend_from_slice(out);
            bytes.extend_from_slice(OSC_D_OK);
        }

        store.feed(&mut scanner, &bytes);
        assert_eq!(store.len(), 2);
        assert_eq!(store.get(0).expect("first").command, "ls");
        assert_eq!(store.get(0).expect("first").output, "a b c\n");
        assert_eq!(store.get(1).expect("second").command, "pwd");
        assert_eq!(store.get(1).expect("second").output, "/tmp\n");
    }

    /// `BlockStore::block_ref` round-trips into a `Selection::Blocks`
    /// payload — the foundation hook the cross-pane verbs follow-up
    /// will lean on.
    #[gpui::test]
    fn block_store_block_ref_round_trips_through_selection(cx: &mut TestAppContext) {
        use codon_pane_bridge::GrammarSelection;

        let pane_id = pane_id(cx);
        let mut store = BlockStore::new(pane_id);
        let mut scanner = Osc133Scanner::new();

        let mut bytes = Vec::new();
        bytes.extend_from_slice(OSC_A);
        bytes.extend_from_slice(OSC_B);
        bytes.extend_from_slice(b"id");
        bytes.extend_from_slice(OSC_C);
        bytes.extend_from_slice(b"uid=1000\n");
        bytes.extend_from_slice(OSC_D_OK);
        store.feed(&mut scanner, &bytes);

        let selection = GrammarSelection::Blocks(vec![store.block_ref(0).expect("ref")]);
        match selection {
            GrammarSelection::Blocks(refs) => {
                assert_eq!(refs.len(), 1);
                assert_eq!(refs[0].pane, pane_id);
                assert_eq!(refs[0].index, 0);
            }
            _ => panic!("expected Selection::Blocks"),
        }
    }

    /// `MockTerminal`-style integration: feed a canned ABCD sequence
    /// and assert the store reports one Block with the right
    /// command + output + exit_status.
    ///
    /// Spec acceptance criterion:
    /// `REQ:codon/terminal-blocks#c-osc-133-parser` →
    /// "Integration test: a `MockTerminal` emits a canned ABCD
    /// sequence with known text; `BlockStore` reports one Block
    /// with the right command + output + exit_status."
    #[gpui::test]
    fn mock_terminal_integration(cx: &mut TestAppContext) {
        let pane_id = pane_id(cx);
        let mut store = BlockStore::new(pane_id);
        let mut scanner = Osc133Scanner::new();

        // Simulate the PTY emitting two chunks across a network-ish
        // seam to exercise the scanner's cross-chunk state.
        store.feed(
            &mut scanner,
            b"some preamble\n\x1b]133;A\x07user$ \x1b]133;B\x07echo ",
        );
        store.feed(&mut scanner, b"hello\x1b]133;C\x07hello\n\x1b]133;D;0\x07");

        assert_eq!(store.len(), 1);
        let block = store.get(0).expect("block");
        assert_eq!(block.command, "echo hello");
        assert_eq!(block.output, "hello\n");
        assert_eq!(block.exit_status, Some(0));
    }

    fn pane_id(cx: &mut TestAppContext) -> EntityId {
        cx.update(|cx| {
            let entity = cx.new(|_| ());
            entity.entity_id()
        })
    }
}
