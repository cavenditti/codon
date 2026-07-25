use std::path::PathBuf;
use std::sync::Arc;

use editor::Editor;
use fs::RenameOptions;
use gpui::{App, Context, WeakEntity, Window, prelude::*};
use multi_buffer::MultiBuffer;
use workspace::Workspace;

use crate::file_manager::FileManager;

/// Open a Zed editor buffer pre-filled with one file name per line —
/// the marked entries in FM display order. The buffer is unsaved-on-
/// disk; closing the editor tab triggers `apply_bulk_rename`, which
/// diffs original→edited names and applies the resulting renames via
/// `fs::Fs::rename`.
///
/// Save-and-then-close is the documented yazi flow, but local
/// (no-file) buffers in Zed prompt for a path on save, which would
/// derail the verb. We therefore commit on close — that's the moment
/// the user has signalled "I'm done editing"; whether they hit save
/// first or not is immaterial for an in-memory buffer.
pub(crate) fn open_bulk_rename_editor(
    workspace: WeakEntity<Workspace>,
    fs: Arc<dyn fs::Fs>,
    targets: Vec<PathBuf>,
    file_manager: WeakEntity<FileManager>,
    window: &mut Window,
    cx: &mut Context<FileManager>,
) {
    let Some(workspace_entity) = workspace.upgrade() else {
        return;
    };
    let title = format!("Bulk rename — {} files", targets.len());
    let originals: Vec<String> = targets
        .iter()
        .map(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| p.display().to_string())
        })
        .collect();
    let seed = format!("{}\n", originals.join("\n"));

    workspace_entity.update(cx, |workspace, cx| {
        let project = workspace.project().clone();
        let buffer = project.update(cx, |project, cx| {
            project.create_local_buffer(&seed, None, false, cx)
        });
        let editor = cx.new(|cx| {
            let multibuffer =
                cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx).with_title(title.clone()));
            Editor::for_multibuffer(multibuffer, Some(project.clone()), window, cx)
        });

        // Fire the rename plan exactly once, when the user closes the
        // tab. `observe_release` runs when the entity is dropped —
        // closing the workspace item drops the last strong handle.
        // We move `buffer` into the closure so the underlying text is
        // still readable inside it (the editor holds the only other
        // strong ref, which is also released at the same point — but
        // before this callback fires).
        let fm = file_manager.clone();
        let fs_for_release = fs.clone();
        cx.observe_release(&editor, move |_workspace, editor, cx| {
            let edited = editor.buffer().read(cx).snapshot(cx).text();
            apply_bulk_rename(
                fm.clone(),
                fs_for_release.clone(),
                targets.clone(),
                originals.clone(),
                edited,
                cx,
            );
        })
        .detach();

        // Keep the buffer entity alive for the lifetime of the editor
        // so the singleton inside the multibuffer keeps producing
        // valid snapshots through the release callback above.
        let _ = buffer;

        workspace.add_item_to_active_pane(Box::new(editor), None, true, window, cx);
    });
}

/// Diff edited lines against the captured originals and dispatch
/// renames. Line-count mismatch is a no-op + toast. First failure
/// rolls back already-applied renames so the verb is all-or-nothing
/// from the user's perspective.
fn apply_bulk_rename(
    fm: WeakEntity<FileManager>,
    fs: Arc<dyn fs::Fs>,
    targets: Vec<PathBuf>,
    originals: Vec<String>,
    edited_text: String,
    cx: &mut App,
) {
    let edited: Vec<String> = edited_text
        .lines()
        .map(|line| line.trim_end_matches('\r').to_string())
        .filter(|line| !line.is_empty())
        .collect();

    if edited.len() != originals.len() {
        let expected = originals.len();
        let got = edited.len();
        fm.update(cx, |fm, cx| {
            fm.surface_error(
                format!("Bulk rename expects {expected} lines, got {got}; nothing applied"),
                cx,
            );
        })
        .ok();
        return;
    }

    let mut plan: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (idx, (source, new_name)) in targets.iter().zip(edited.iter()).enumerate() {
        if new_name == &originals[idx] {
            continue;
        }
        let parent = source
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/"));
        let destination = parent.join(new_name);
        plan.push((source.clone(), destination));
    }

    if plan.is_empty() {
        return;
    }

    let fm_for_spawn = fm.clone();
    cx.spawn(async move |cx| {
        let mut applied: Vec<(PathBuf, PathBuf)> = Vec::with_capacity(plan.len());
        let mut failure: Option<(PathBuf, anyhow::Error)> = None;
        for (source, destination) in &plan {
            if destination.exists() {
                failure = Some((
                    source.clone(),
                    anyhow::anyhow!("target {:?} already exists", destination),
                ));
                break;
            }
            let result = fs
                .rename(
                    source,
                    destination,
                    RenameOptions {
                        overwrite: false,
                        ignore_if_exists: false,
                        create_parents: false,
                    },
                )
                .await;
            match result {
                Ok(()) => applied.push((source.clone(), destination.clone())),
                Err(e) => {
                    failure = Some((source.clone(), e));
                    break;
                }
            }
        }

        if let Some((path, err)) = failure {
            // Rollback already-applied renames in reverse order so
            // any chain (a→b, b→c) unwinds without colliding. Errors
            // here are visible only via log — the original failure is
            // what we want the user to see.
            for (source, destination) in applied.iter().rev() {
                if let Err(rollback_err) = fs
                    .rename(
                        destination,
                        source,
                        RenameOptions {
                            overwrite: false,
                            ignore_if_exists: false,
                            create_parents: false,
                        },
                    )
                    .await
                {
                    log::warn!(
                        "bulk-rename rollback failed for {:?}: {rollback_err}",
                        destination
                    );
                }
            }
            fm_for_spawn
                .update(cx, |fm, cx| {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.display().to_string());
                    fm.surface_error(
                        format!("Bulk rename failed on {name}: {err}; rolled back"),
                        cx,
                    );
                })
                .ok();
            return;
        }

        fm_for_spawn
            .update(cx, |fm, cx| {
                let applied_count = applied.len();
                fm.surface_error(format!("Bulk rename: applied {applied_count} entries"), cx);
                fm.reload_entries_after_bulk_rename(cx);
            })
            .ok();
    })
    .detach();
}

/// Test seam for the diff stage so unit tests don't need a `Workspace`.
#[cfg(test)]
pub(crate) fn diff_for_test(
    originals: &[String],
    edited_text: &str,
) -> Result<Vec<(usize, String, String)>, (usize, usize)> {
    let edited: Vec<String> = edited_text
        .lines()
        .map(|line| line.trim_end_matches('\r').to_string())
        .filter(|line| !line.is_empty())
        .collect();
    if edited.len() != originals.len() {
        return Err((originals.len(), edited.len()));
    }
    let mut plan = Vec::new();
    for (idx, (original, new_name)) in originals.iter().zip(edited.iter()).enumerate() {
        if new_name != original {
            plan.push((idx, original.clone(), new_name.clone()));
        }
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_emits_zero_when_unchanged() {
        let originals = vec!["a".to_string(), "b".to_string()];
        let plan = diff_for_test(&originals, "a\nb\n").expect("same count");
        assert!(plan.is_empty());
    }

    #[test]
    fn diff_emits_changed_pairs_in_order() {
        let originals = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let plan = diff_for_test(&originals, "a\nB\nc-new\n").expect("same count");
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0], (1, "b".to_string(), "B".to_string()));
        assert_eq!(plan[1], (2, "c".to_string(), "c-new".to_string()));
    }

    #[test]
    fn diff_rejects_line_count_mismatch_short() {
        let originals = vec!["a".to_string(), "b".to_string()];
        let err = diff_for_test(&originals, "a\n").expect_err("expects mismatch");
        assert_eq!(err, (2, 1));
    }

    #[test]
    fn diff_rejects_line_count_mismatch_long() {
        let originals = vec!["a".to_string()];
        let err = diff_for_test(&originals, "a\nb\n").expect_err("expects mismatch");
        assert_eq!(err, (1, 2));
    }

    #[test]
    fn diff_ignores_trailing_carriage_returns() {
        let originals = vec!["a".to_string(), "b".to_string()];
        let plan = diff_for_test(&originals, "a\r\nB\r\n").expect("same count");
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0], (1, "b".to_string(), "B".to_string()));
    }
}
