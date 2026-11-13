/// A WebView blur is not proof that the user left the popup. Only dismiss when
/// the native foreground root is known to belong to another window.
pub fn should_hide_on_blur(pinned: bool, collection_open: bool, foreground: Option<bool>) -> bool {
    !pinned && !collection_open && foreground == Some(false)
}

/// The screenshot overlay closes when the user leaves it, but a blur alone does not say so:
/// while the overlay is being shown, focus moves from the window to the WebView inside it and
/// the window is told it lost focus although it is still the one in front. That happened on
/// the first opening after a start, and the overlay closed itself at once.
///
/// Unlike the popup, an overlay that covers the screen must not stay open by mistake, so
/// when the foreground window cannot be determined it closes.
pub fn should_hide_overlay_on_blur(had_focus: bool, foreground: Option<bool>) -> bool {
    had_focus && foreground != Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_survives_focus_moving_into_its_own_webview() {
        assert!(!should_hide_overlay_on_blur(true, Some(true)));
    }

    #[test]
    fn overlay_closes_when_another_window_is_in_front_or_nothing_is_known() {
        assert!(should_hide_overlay_on_blur(true, Some(false)));
        assert!(should_hide_overlay_on_blur(true, None));
    }

    #[test]
    fn overlay_that_never_had_focus_is_not_closed_by_a_stray_blur() {
        for foreground in [Some(true), Some(false), None] {
            assert!(!should_hide_overlay_on_blur(false, foreground));
        }
    }
    #[test]
    fn native_drag_and_resize_focus_transfer_do_not_hide_unpinned_popup() {
        assert!(!should_hide_on_blur(false, false, Some(true)));
    }

    #[test]
    fn outside_click_or_alt_tab_dismisses_only_an_unprotected_popup() {
        for pinned in [false, true] {
            for collection_open in [false, true] {
                assert_eq!(
                    should_hide_on_blur(pinned, collection_open, Some(false)),
                    !pinned && !collection_open
                );
            }
        }
    }

    #[test]
    fn unknown_foreground_does_not_spontaneously_dismiss_the_popup() {
        assert!(!should_hide_on_blur(false, false, None));
    }
}
