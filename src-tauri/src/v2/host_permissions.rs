//! Gate for future Windows host tools. This module DOES NOT run commands,
//! fetch URLs, or bypass the separate Cowork VM. Consent comes from a trusted
//! host UI, never from a model-supplied tool argument.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostAction {
    ReadWorkspace,
    WriteWorkspace,
    ExecuteShell,
    FetchPublicWeb,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct HostIntent {
    pub action: HostAction,
    pub target: String,
    pub working_directory: PathBuf,
}

struct Pending {
    intent: HostIntent,
    expires: Instant,
    approved: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PermissionError {
    OutOfScope,
    Disabled,
    TooManyPending,
    NotFound,
    NotApproved,
    Expired,
    MismatchedRequest,
}

pub(crate) struct HostPermissionGate {
    workspace_root: PathBuf,
    pending: HashMap<String, Pending>,
    ttl: Duration,
    enable_shell: bool,
    enable_network: bool,
}

impl HostPermissionGate {
    pub(crate) fn new(
        workspace_root: PathBuf,
        ttl: Duration,
        enable_shell: bool,
        enable_network: bool,
    ) -> Result<Self, PermissionError> {
        let workspace_root = workspace_root
            .canonicalize()
            .map_err(|_| PermissionError::OutOfScope)?;
        if !workspace_root.is_dir() {
            return Err(PermissionError::OutOfScope);
        }
        Ok(Self {
            workspace_root,
            pending: HashMap::new(),
            ttl,
            enable_shell,
            enable_network,
        })
    }

    /// The caller shows this ID and exact requested action to the local user.
    /// The proposed request is NOT authorized until a trusted UI approves it.
    pub(crate) fn propose(&mut self, intent: HostIntent) -> Result<String, PermissionError> {
        self.verify_scope(&intent)?;
        self.prune();
        if self.pending.len() >= 128 {
            return Err(PermissionError::TooManyPending);
        }
        let id = uuid::Uuid::new_v4().simple().to_string();
        self.pending.insert(
            id.clone(),
            Pending {
                intent,
                expires: Instant::now() + self.ttl,
                approved: false,
            },
        );
        Ok(id)
    }

    // MUST only be invoked by a trusted host UI action after the user has
    // reviewed the exact command/URL. Do not expose this through MCP or HTTP.
    pub(crate) fn approve_from_host_ui(&mut self, id: &str) -> Result<(), PermissionError> {
        let pending = self.pending.get_mut(id).ok_or(PermissionError::NotFound)?;
        if Instant::now() >= pending.expires {
            self.pending.remove(id);
            return Err(PermissionError::Expired);
        }
        pending.approved = true;
        Ok(())
    }

    /// Single-use authorization; even a matching model retry must seek fresh
    /// consent. No command execution is implemented in this module.
    pub(crate) fn consume(
        &mut self,
        id: &str,
        exact_intent: &HostIntent,
    ) -> Result<(), PermissionError> {
        let pending = self.pending.remove(id).ok_or(PermissionError::NotFound)?;
        if Instant::now() >= pending.expires {
            return Err(PermissionError::Expired);
        }
        if !pending.approved {
            return Err(PermissionError::NotApproved);
        }
        if &pending.intent != exact_intent {
            return Err(PermissionError::MismatchedRequest);
        }
        self.verify_scope(exact_intent)
    }

    fn prune(&mut self) {
        let now = Instant::now();
        self.pending.retain(|_, p| p.expires > now);
    }

    fn verify_scope(&self, intent: &HostIntent) -> Result<(), PermissionError> {
        if intent.target.trim().is_empty() {
            return Err(PermissionError::OutOfScope);
        }
        let directory = intent
            .working_directory
            .canonicalize()
            .map_err(|_| PermissionError::OutOfScope)?;
        if !directory.starts_with(&self.workspace_root) {
            return Err(PermissionError::OutOfScope);
        }
        if matches!(
            intent.action,
            HostAction::ReadWorkspace | HostAction::WriteWorkspace
        ) {
            let name = Path::new(&intent.target);
            let full = if name.is_absolute() {
                name.to_path_buf()
            } else {
                directory.join(name)
            };
            // Reads must exist. For a new write, the existing parent must be
            // inside the workspace; for an existing write, resolve symlinks.
            let resolved = if full.exists() || intent.action == HostAction::ReadWorkspace {
                full.canonicalize()
                    .map_err(|_| PermissionError::OutOfScope)?
            } else {
                full.parent()
                    .ok_or(PermissionError::OutOfScope)?
                    .canonicalize()
                    .map_err(|_| PermissionError::OutOfScope)?
            };
            if !resolved.starts_with(&self.workspace_root) {
                return Err(PermissionError::OutOfScope);
            }
        }
        if intent.action == HostAction::ExecuteShell && !self.enable_shell {
            return Err(PermissionError::Disabled);
        }
        if intent.action == HostAction::FetchPublicWeb && !self.enable_network {
            return Err(PermissionError::Disabled);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(root: &Path, action: HostAction) -> HostIntent {
        HostIntent {
            action,
            target: "cargo test".into(),
            working_directory: root.to_path_buf(),
        }
    }

    #[test]
    fn shell_requires_both_enablement_and_explicit_approval() {
        let root = std::env::temp_dir();
        let mut gate =
            HostPermissionGate::new(root.clone(), Duration::from_secs(60), false, false).unwrap();
        assert_eq!(
            gate.propose(request(&root, HostAction::ExecuteShell)),
            Err(PermissionError::Disabled)
        );
        let mut gate =
            HostPermissionGate::new(root.clone(), Duration::from_secs(60), true, false).unwrap();
        let intent = request(&root, HostAction::ExecuteShell);
        let id = gate.propose(intent.clone()).unwrap();
        assert_eq!(
            gate.consume(&id, &intent),
            Err(PermissionError::NotApproved)
        );
        let id = gate.propose(intent.clone()).unwrap();
        gate.approve_from_host_ui(&id).unwrap();
        assert_eq!(gate.consume(&id, &intent), Ok(()));
        assert_eq!(gate.consume(&id, &intent), Err(PermissionError::NotFound));
    }

    #[test]
    fn consent_cannot_be_replayed_for_modified_command() {
        let root = std::env::temp_dir();
        let mut gate =
            HostPermissionGate::new(root.clone(), Duration::from_secs(60), true, false).unwrap();
        let intent = request(&root, HostAction::ExecuteShell);
        let id = gate.propose(intent.clone()).unwrap();
        gate.approve_from_host_ui(&id).unwrap();
        let mut changed = intent.clone();
        changed.target = "different command".into();
        assert_eq!(
            gate.consume(&id, &changed),
            Err(PermissionError::MismatchedRequest)
        );
        assert_eq!(gate.consume(&id, &intent), Err(PermissionError::NotFound));
    }

    #[test]
    fn expired_grants_cannot_authorize_actions() {
        let root = std::env::temp_dir();
        let mut gate = HostPermissionGate::new(root.clone(), Duration::ZERO, true, false).unwrap();
        let intent = request(&root, HostAction::ExecuteShell);
        let id = gate.propose(intent).unwrap();
        assert_eq!(
            gate.approve_from_host_ui(&id),
            Err(PermissionError::Expired)
        );
    }

    #[test]
    fn workspace_read_cannot_escape_via_absolute_path() {
        let root = std::env::temp_dir();
        let parent = root.parent().unwrap().to_path_buf();
        let mut gate =
            HostPermissionGate::new(root.clone(), Duration::from_secs(60), false, false).unwrap();
        let mut intent = request(&root, HostAction::ReadWorkspace);
        intent.target = parent.to_string_lossy().to_string();
        assert_eq!(gate.propose(intent), Err(PermissionError::OutOfScope));
    }

    #[test]
    fn writing_through_parent_directory_escape_is_denied() {
        let root = std::env::temp_dir();
        let mut gate =
            HostPermissionGate::new(root.clone(), Duration::from_secs(60), false, false).unwrap();
        let mut intent = request(&root, HostAction::WriteWorkspace);
        intent.target = "../outside-test-file.txt".into();
        assert_eq!(gate.propose(intent), Err(PermissionError::OutOfScope));
    }

    #[test]
    fn no_host_network_access_by_default() {
        let root = std::env::temp_dir();
        let mut gate =
            HostPermissionGate::new(root.clone(), Duration::from_secs(60), false, false).unwrap();
        let mut intent = request(&root, HostAction::FetchPublicWeb);
        intent.target = "https://example.com".into();
        assert_eq!(gate.propose(intent), Err(PermissionError::Disabled));
    }
}
