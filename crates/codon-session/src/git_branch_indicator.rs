use gpui::{
    Action as _, Context, Empty, Entity, IntoElement, Render, SharedString, Subscription,
    WeakEntity, Window,
};
use project::Project;
use project::git_store::GitStoreEvent;
use ui::{Button, ButtonCommon as _, Clickable as _, Color, LabelSize, Tooltip};
use workspace::{ItemHandle, StatusItemView, Workspace};

pub struct GitBranchIndicator {
    workspace: WeakEntity<Workspace>,
    project: Entity<Project>,
    branch: Option<SharedString>,
    _git_store_subscription: Subscription,
}

impl GitBranchIndicator {
    pub fn new(workspace: &Workspace, cx: &mut Context<Self>) -> Self {
        let project = workspace.project().clone();
        let git_store = project.read(cx).git_store().clone();
        let _git_store_subscription =
            cx.subscribe(&git_store, |this, _, event, cx| match event {
                GitStoreEvent::RepositoryUpdated(..)
                | GitStoreEvent::RepositoryAdded
                | GitStoreEvent::RepositoryRemoved(_)
                | GitStoreEvent::ActiveRepositoryChanged(_) => {
                    this.refresh(cx);
                }
                _ => {}
            });

        let mut this = Self {
            workspace: workspace.weak_handle(),
            project,
            branch: None,
            _git_store_subscription,
        };
        this.refresh(cx);
        this
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let new_branch = self
            .project
            .read(cx)
            .active_repository(cx)
            .and_then(|repo| repo.read(cx).branch.as_ref().map(|b| b.name().to_string()))
            .map(SharedString::from);
        if new_branch != self.branch {
            self.branch = new_branch;
            cx.notify();
        }
    }
}

impl Render for GitBranchIndicator {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let Some(branch) = self.branch.clone() else {
            return Empty.into_any_element();
        };
        let label = SharedString::from(format!(" {}", branch));
        let workspace = self.workspace.clone();

        Button::new("git-branch-indicator", label)
            .label_size(LabelSize::Small)
            .color(Color::Muted)
            .tooltip(|_window, cx| {
                Tooltip::for_action("Switch Branch", &zed_actions::git::Branch, cx)
            })
            .on_click(move |_, window, cx| {
                if let Some(workspace) = workspace.upgrade() {
                    workspace.update(cx, |_, cx| {
                        window.dispatch_action(zed_actions::git::Branch.boxed_clone(), cx);
                    });
                }
            })
            .into_any_element()
    }
}

impl StatusItemView for GitBranchIndicator {
    fn set_active_pane_item(
        &mut self,
        _active_pane_item: Option<&dyn ItemHandle>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh(cx);
    }
}
