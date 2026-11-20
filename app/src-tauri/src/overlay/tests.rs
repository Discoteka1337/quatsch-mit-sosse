use super::*;

#[test]
fn ready_preference_only_applies_while_idle() {
    assert!(matches!(
        OverlayState::Ready.view(false, false),
        View::Hidden
    ));
    assert!(matches!(OverlayState::Ready.view(true, false), View::Ready));
    assert!(matches!(
        OverlayState::Listening { level: 120 }.view(false, false),
        View::Listening { level: 100, .. }
    ));
    assert!(
        matches!(OverlayState::Processing.view(false, false), View::Status(s) if s == messages().overlay_processing)
    );
    assert!(
        matches!(OverlayState::Inserting.view(false, false), View::Status(s) if s == messages().overlay_inserting)
    );
    assert!(
        matches!(OverlayState::Administrator.view(false, false), View::Administrator { label, detail }
        if label == messages().overlay_administrator && detail == messages().overlay_administrator_help)
    );
}

#[test]
fn recovery_keeps_the_original_text_available_after_a_clipboard_failure() {
    let status = OverlayState::Error {
        message: "Einfügen fehlgeschlagen".into(),
        text: "Äpfel & Öl".into(),
    };
    assert!(
        matches!(status.view(false, false), View::Error { label, copy }
        if label == "Einfügen fehlgeschlagen" && copy == messages().overlay_copy)
    );
    assert!(matches!(status.view(false, true), View::Error { label, .. }
        if label == messages().overlay_copy_error));
    assert_eq!(status.copy_text(), Some("Äpfel & Öl"));
    assert_eq!(OverlayState::Ready.copy_text(), None);
}
