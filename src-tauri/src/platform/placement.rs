#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PopupPlacement {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkArea {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

pub fn place_popup(
    cursor_x: i32,
    cursor_y: i32,
    width: i32,
    height: i32,
    work_area: WorkArea,
) -> PopupPlacement {
    PopupPlacement {
        x: place_axis(cursor_x, width, work_area.left, work_area.right, 12, 12),
        y: place_axis(cursor_y, height, work_area.top, work_area.bottom, 18, 12),
    }
}

fn place_axis(
    cursor: i32,
    size: i32,
    start: i32,
    end: i32,
    after_gap: i32,
    before_gap: i32,
) -> i32 {
    let start = i64::from(start);
    let end = i64::from(end).max(start);
    let size = i64::from(size.max(1));
    let cursor = i64::from(cursor);

    let available = end - start;
    if size >= available {
        return start as i32;
    }

    let max_start = end - size;
    let after = cursor + i64::from(after_gap);
    if (start..=max_start).contains(&after) {
        return after as i32;
    }

    let before = cursor - size - i64::from(before_gap);
    if (start..=max_start).contains(&before) {
        return before as i32;
    }

    after.clamp(start, max_start) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIMARY: WorkArea = WorkArea {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    };

    #[test]
    fn places_popup_after_cursor_when_there_is_room() {
        assert_eq!(
            place_popup(400, 300, 420, 260, PRIMARY),
            PopupPlacement { x: 412, y: 318 }
        );
    }

    #[test]
    fn moves_popup_to_left_of_right_edge() {
        assert_eq!(
            place_popup(1900, 300, 420, 260, PRIMARY),
            PopupPlacement { x: 1468, y: 318 }
        );
    }

    #[test]
    fn moves_popup_above_bottom_edge() {
        assert_eq!(
            place_popup(400, 1030, 420, 260, PRIMARY),
            PopupPlacement { x: 412, y: 758 }
        );
    }

    #[test]
    fn supports_secondary_monitors_with_negative_coordinates() {
        let secondary = WorkArea {
            left: -1280,
            top: 0,
            right: 0,
            bottom: 984,
        };

        assert_eq!(
            place_popup(-20, 970, 420, 260, secondary),
            PopupPlacement { x: -452, y: 698 }
        );
    }

    #[test]
    fn anchors_oversized_popup_to_work_area_origin() {
        let small = WorkArea {
            left: 100,
            top: 50,
            right: 400,
            bottom: 250,
        };

        assert_eq!(
            place_popup(390, 240, 420, 260, small),
            PopupPlacement { x: 100, y: 50 }
        );
    }
}
