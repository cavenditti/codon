use std::path::PathBuf;

use db::{
    query,
    sqlez::{domain::Domain, thread_safe_connection::ThreadSafeConnection},
    sqlez_macros::sql,
};
use workspace::{ItemId, WorkspaceDb, WorkspaceId};

pub struct FileManagerDb(ThreadSafeConnection);

impl Domain for FileManagerDb {
    const NAME: &str = stringify!(FileManagerDb);

    const MIGRATIONS: &[&str] = &[sql!(
            CREATE TABLE file_managers (
                workspace_id INTEGER,
                item_id INTEGER,
                current_dir BLOB,
                PRIMARY KEY(workspace_id, item_id),
                FOREIGN KEY(workspace_id) REFERENCES workspaces(workspace_id)
                ON DELETE CASCADE
            ) STRICT;
    )];
}

db::static_connection!(FileManagerDb, [WorkspaceDb]);

impl FileManagerDb {
    query! {
        pub async fn save_current_dir(
            item_id: ItemId,
            workspace_id: WorkspaceId,
            current_dir: PathBuf
        ) -> Result<()> {
            INSERT OR REPLACE INTO file_managers(item_id, workspace_id, current_dir)
            VALUES (?, ?, ?)
        }
    }

    query! {
        pub fn get_current_dir(item_id: ItemId, workspace_id: WorkspaceId) -> Result<Option<PathBuf>> {
            SELECT current_dir
            FROM file_managers
            WHERE item_id = ? AND workspace_id = ?
        }
    }
}
