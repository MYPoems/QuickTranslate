/// A WebView blur is not proof that the user left the popup. Only dismiss when
/// the native foreground root is known to belong to another window.
pub fn should_hide_on_blur(pinned: bool, collection_open: bool, foreground: Option<bool>) -> bool {
    !pinned && !collection_open && foreground == Some(false)
}

#[cfg(test)]
mod tests {
    use super::*;

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
