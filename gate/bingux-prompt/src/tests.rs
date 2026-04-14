#[cfg(test)]
mod tests {
    use crate::backend::{AutoAllowBackend, AutoDenyBackend};
    use crate::history::PromptHistory;
    use crate::server::PromptServer;
    use crate::types::{PromptRequest, PromptResponse, ResourceType};

    fn make_request(name: &str, resource: ResourceType, dangerous: bool) -> PromptRequest {
        PromptRequest {
            id: 0,
            package_name: name.to_string(),
            package_icon: None,
            resource_type: resource,
            resource_detail: "/tmp/test".to_string(),
            is_dangerous: dangerous,
            timestamp: 1000,
        }
    }

    // -----------------------------------------------------------------------
    // ResourceType
    // -----------------------------------------------------------------------

    #[test]
    fn resource_type_descriptions_are_non_empty() {
        let variants = [
            ResourceType::FileRead,
            ResourceType::FileWrite,
            ResourceType::FileList,
            ResourceType::NetworkOutbound,
            ResourceType::NetworkListen,
            ResourceType::DeviceGpu,
            ResourceType::DeviceAudio,
            ResourceType::DeviceCamera,
            ResourceType::DeviceInput,
            ResourceType::Display,
            ResourceType::Clipboard,
            ResourceType::Notifications,
            ResourceType::ProcessExec,
            ResourceType::ProcessPtrace,
            ResourceType::DbusSession,
            ResourceType::DbusSystem,
            ResourceType::Mount,
        ];
        for v in &variants {
            assert!(!v.description().is_empty(), "{v:?} has empty description");
            assert!(!v.icon_name().is_empty(), "{v:?} has empty icon name");
        }
    }

    // -----------------------------------------------------------------------
    // PromptRequest formatting
    // -----------------------------------------------------------------------

    #[test]
    fn format_message_contains_package_and_resource() {
        let req = make_request("com.example.App", ResourceType::FileRead, false);
        let msg = req.format_message();
        assert!(msg.contains("com.example.App"));
        assert!(msg.contains("Read files"));
        assert!(msg.contains("/tmp/test"));
    }

    // -----------------------------------------------------------------------
    // Available responses
    // -----------------------------------------------------------------------

    #[test]
    fn dangerous_request_excludes_always_allow() {
        let req = make_request("test", ResourceType::ProcessPtrace, true);
        let responses = req.available_responses();
        assert!(responses.contains(&PromptResponse::Deny));
        assert!(responses.contains(&PromptResponse::AllowOnce));
        assert!(!responses.contains(&PromptResponse::AlwaysAllow));
    }

    #[test]
    fn normal_request_has_all_responses() {
        let req = make_request("test", ResourceType::FileRead, false);
        let responses = req.available_responses();
        assert!(responses.contains(&PromptResponse::Deny));
        assert!(responses.contains(&PromptResponse::AllowOnce));
        assert!(responses.contains(&PromptResponse::AlwaysAllow));
    }

    // -----------------------------------------------------------------------
    // PromptHistory
    // -----------------------------------------------------------------------

    #[test]
    fn history_record_and_recent() {
        let mut history = PromptHistory::new(10);
        let req = make_request("pkg-a", ResourceType::FileRead, false);
        history.record(req, PromptResponse::AllowOnce);

        assert_eq!(history.recent(10).len(), 1);
        assert_eq!(history.recent(10)[0].response, PromptResponse::AllowOnce);
    }

    #[test]
    fn history_evicts_oldest_when_full() {
        let mut history = PromptHistory::new(2);
        history.record(
            make_request("a", ResourceType::FileRead, false),
            PromptResponse::Deny,
        );
        history.record(
            make_request("b", ResourceType::FileRead, false),
            PromptResponse::AllowOnce,
        );
        history.record(
            make_request("c", ResourceType::FileRead, false),
            PromptResponse::AlwaysAllow,
        );

        let entries = history.recent(10);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].request.package_name, "b");
        assert_eq!(entries[1].request.package_name, "c");
    }

    #[test]
    fn history_clear() {
        let mut history = PromptHistory::new(10);
        history.record(
            make_request("a", ResourceType::FileRead, false),
            PromptResponse::Deny,
        );
        history.clear();
        assert_eq!(history.recent(10).len(), 0);
    }

    #[test]
    fn history_for_package() {
        let mut history = PromptHistory::new(10);
        history.record(
            make_request("alpha", ResourceType::FileRead, false),
            PromptResponse::Deny,
        );
        history.record(
            make_request("beta", ResourceType::FileWrite, false),
            PromptResponse::AllowOnce,
        );
        history.record(
            make_request("alpha", ResourceType::NetworkOutbound, false),
            PromptResponse::AlwaysAllow,
        );

        let alpha = history.for_package("alpha");
        assert_eq!(alpha.len(), 2);

        let beta = history.for_package("beta");
        assert_eq!(beta.len(), 1);

        let none = history.for_package("gamma");
        assert!(none.is_empty());
    }

    // -----------------------------------------------------------------------
    // AutoDenyBackend
    // -----------------------------------------------------------------------

    #[test]
    fn auto_deny_backend_returns_deny() {
        let backend = AutoDenyBackend;
        let req = make_request("test", ResourceType::FileRead, false);
        let resp = backend.show_prompt(&req).unwrap();
        assert_eq!(resp, PromptResponse::Deny);
    }

    // -----------------------------------------------------------------------
    // AutoAllowBackend
    // -----------------------------------------------------------------------

    #[test]
    fn auto_allow_backend_returns_allow_once() {
        let backend = AutoAllowBackend;
        let req = make_request("test", ResourceType::FileRead, false);
        let resp = backend.show_prompt(&req).unwrap();
        assert_eq!(resp, PromptResponse::AllowOnce);
    }

    // -----------------------------------------------------------------------
    // PromptServer with AutoDenyBackend
    // -----------------------------------------------------------------------

    #[test]
    fn server_auto_deny_submit_returns_deny_and_records_history() {
        let mut server = PromptServer::new(Box::new(AutoDenyBackend));
        let req = make_request("pkg", ResourceType::FileRead, false);
        let resp = server.submit(req).unwrap();
        assert_eq!(resp, PromptResponse::Deny);

        let history = server.history().recent(10);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].response, PromptResponse::Deny);
    }

    // -----------------------------------------------------------------------
    // PromptServer with AutoAllowBackend
    // -----------------------------------------------------------------------

    #[test]
    fn server_auto_allow_submit_returns_allow_once() {
        let mut server = PromptServer::new(Box::new(AutoAllowBackend));
        let req = make_request("pkg", ResourceType::FileRead, false);
        let resp = server.submit(req).unwrap();
        assert_eq!(resp, PromptResponse::AllowOnce);
    }

    // -----------------------------------------------------------------------
    // PromptServer dismiss
    // -----------------------------------------------------------------------

    #[test]
    fn server_dismiss_removes_pending() {
        let mut server = PromptServer::new(Box::new(AutoDenyBackend));

        // After submit completes, the prompt is no longer pending.
        let req = make_request("pkg", ResourceType::FileRead, false);
        let _ = server.submit(req);
        assert!(!server.is_pending(1));

        // Dismissing a non-existent ID is a no-op.
        server.dismiss(999);
    }

    // We need to import the trait to call show_prompt directly.
    use crate::backend::PromptBackend;
}
