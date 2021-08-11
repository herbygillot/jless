use super::flatjson::{FlatJson, Index, OptionIndex};

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum Mode {
    Line,
    Data,
}

const DEFAULT_SCROLLOFF: u16 = 3;
const DEFAULT_HEIGHT: u16 = 24;

pub struct JsonViewer {
    flatjson: FlatJson,
    top_row: Index,
    focused_row: Index,

    height: u16,
    // We call this scrolloff_setting, to differentiate between
    // what it's set to, and what the scrolloff functionally is
    // if it's set to value >= height / 2.
    //
    // Access the functional value via .scrolloff().
    scrolloff_setting: u16,
    mode: Mode,
}

impl JsonViewer {
    fn new(flatjson: FlatJson, mode: Mode) -> JsonViewer {
        JsonViewer {
            flatjson,
            top_row: 0,
            focused_row: 0,
            height: DEFAULT_HEIGHT,
            scrolloff_setting: DEFAULT_SCROLLOFF,
            mode,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum Action {
    MoveUp(usize),
    MoveDown(usize),
    MoveLeft,
    MoveRight,

    ToggleCollapsed,

    FocusFirstElem,
    FocusLastElem,
    FocusTop,
    FocusBottom,

    ScrollUp(usize),
    ScrollDown(usize),

    ToggleMode,
}

impl JsonViewer {
    fn perform_action(&mut self, action: Action) {
        let track_window = JsonViewer::should_refocus_window(&action);

        match action {
            Action::MoveUp(n) => self.move_up(n),
            Action::MoveDown(n) => self.move_down(n),
            Action::MoveLeft => self.move_left(),
            Action::MoveRight => self.move_right(),
            Action::ScrollUp(n) => self.scroll_up(n),
            Action::ScrollDown(n) => self.scroll_down(n),
            Action::ToggleMode => {
                // TODO: custom window management here
                self.toggle_mode();
            }
            _ => {}
        }

        if track_window {
            self.ensure_focused_row_is_visible();
        }
    }

    fn should_refocus_window(action: &Action) -> bool {
        match action {
            Action::MoveUp(_) => true,
            Action::MoveDown(_) => true,
            Action::MoveLeft => true,
            Action::MoveRight => true,
            Action::ScrollUp(_) => false,
            Action::ScrollDown(_) => false,
            Action::ToggleMode => false,
            _ => false,
        }
    }

    fn move_up(&mut self, rows: usize) {
        let mut row = self.focused_row;

        for _ in 0..rows {
            let prev_row = match self.mode {
                Mode::Line => self.flatjson.prev_visible_row(row),
                Mode::Data => self.flatjson.prev_item(row),
            };

            match prev_row {
                OptionIndex::Nil => break,
                OptionIndex::Index(prev_row_index) => {
                    row = prev_row_index;
                }
            }
        }

        self.focused_row = row;
    }

    fn move_down(&mut self, rows: usize) {
        let mut row = self.focused_row;

        for _ in 0..rows {
            let next_row = match self.mode {
                Mode::Line => self.flatjson.next_visible_row(row),
                Mode::Data => self.flatjson.next_item(row),
            };

            match next_row {
                OptionIndex::Nil => break,
                OptionIndex::Index(next_row_index) => {
                    row = next_row_index;
                }
            }
        }

        self.focused_row = row;
    }

    fn move_right(&mut self) {
        let focused_row = &self.flatjson[self.focused_row];
        if focused_row.is_primitive() {
            return;
        }

        if focused_row.is_collapsed() {
            self.flatjson.expand(self.focused_row);
            return;
        }

        if focused_row.is_opening_of_container() {
            self.focused_row = focused_row.first_child().unwrap();
        } else {
            debug_assert!(
                self.mode == Mode::Line,
                "Can't be focused on closing char in Data mode"
            );
            self.focused_row = self.flatjson.prev_visible_row(self.focused_row).unwrap();
        }
    }

    fn move_left(&mut self) {
        if self.flatjson[self.focused_row].is_container()
            && self.flatjson[self.focused_row].is_expanded()
        {
            self.flatjson.collapse(self.focused_row);
            if self.flatjson[self.focused_row].is_closing_of_container() {
                self.focused_row = self.flatjson[self.focused_row].pair_index().unwrap();
            }
            return;
        }

        if let OptionIndex::Index(parent) = self.flatjson[self.focused_row].parent {
            self.focused_row = parent;
        }
    }

    fn scroll_up(&mut self, rows: usize) {
        self.top_row = self.count_n_lines_before(self.top_row, rows, self.mode);
        let max_focused_row = self.count_n_lines_past(
            self.top_row,
            (self.height - self.scrolloff() - 1) as usize,
            self.mode,
        );

        if self.focused_row > max_focused_row {
            self.focused_row = max_focused_row;
        }
    }

    fn scroll_down(&mut self, rows: usize) {
        self.top_row = self.count_n_lines_past(self.top_row, rows, self.mode);
        let first_focusable_row =
            self.count_n_lines_past(self.top_row, self.scrolloff() as usize, self.mode);

        if self.focused_row < first_focusable_row {
            self.focused_row = first_focusable_row;
        }
    }

    fn toggle_collapsed(&mut self) {
        let focused_row = &mut self.flatjson[self.focused_row];
        if focused_row.is_primitive() {
            return;
        }

        if focused_row.is_closing_of_container() {
            debug_assert!(
                focused_row.is_expanded(),
                "Focused on closing char when row is collapsed",
            );
            self.focused_row = self.flatjson[self.focused_row].pair_index().unwrap();
        }

        self.flatjson.toggle_collapsed(self.focused_row);
    }

    fn toggle_mode(&mut self) {
        // If toggling from line mode to data mode, and the cursor is currently and a closing
        // brace, move the cursor up to the last visible child.
        self.mode = match self.mode {
            Mode::Line => Mode::Data,
            Mode::Data => Mode::Line,
        }
    }

    fn scrolloff(&self) -> u16 {
        self.scrolloff_setting.min((self.height - 1) / 2)
    }

    // This is called after moving the cursor up or down (or other operations that
    // change where the focused row is) and makes sure that it isn't within SCROLLOFF
    // lines of the top or bottom of the screen.
    fn ensure_focused_row_is_visible(&mut self) {
        println!("Ensuring!");
        // height; scrolloff; actual scrolloff; max_visible
        //   100       3              3            96
        //   15        7              7             7
        //   15        8              7             7
        //   16        8              7             8
        let scrolloff = self.scrolloff();
        let max_visible = self.height - scrolloff - 1;

        let num_visible_before_focused = self.count_visible_rows_before(
            self.top_row,
            self.focused_row,
            // Add 1 so we can differentiate between == max_visible and > max_visible
            max_visible + 1,
            self.mode,
        );

        if num_visible_before_focused < scrolloff {
            self.top_row =
                self.count_n_lines_before(self.focused_row, scrolloff as usize, self.mode)
        } else if num_visible_before_focused > max_visible {
            let last_line = match self.mode {
                Mode::Line => self.flatjson.last_visible_index(),
                Mode::Data => self.flatjson.last_visible_item(),
            };
            let lines_visible_before_eof = self.count_visible_rows_before(
                self.focused_row,
                last_line,
                scrolloff + 1,
                self.mode,
            );
            let bottom_padding = scrolloff.min(lines_visible_before_eof);
            println!(
                "lines_visible_before_eof: {}, bottom_padding: {}",
                lines_visible_before_eof, bottom_padding
            );
            self.top_row = self.count_n_lines_before(
                self.focused_row,
                (self.height - bottom_padding - 1) as usize,
                self.mode,
            )
        }
    }

    fn count_n_lines_before(&self, mut start: Index, mut lines: usize, mode: Mode) -> Index {
        while lines != 0 && start != 0 {
            start = match mode {
                Mode::Line => self.flatjson.prev_visible_row(start).unwrap(),
                Mode::Data => self.flatjson.prev_item(start).unwrap(),
            };
            lines -= 1;
        }
        start
    }

    fn count_n_lines_past(&self, mut start: Index, mut lines: usize, mode: Mode) -> Index {
        while lines != 0 {
            let next = match self.mode {
                Mode::Line => self.flatjson.next_visible_row(start),
                Mode::Data => self.flatjson.next_item(start),
            };

            match next {
                OptionIndex::Nil => break,
                OptionIndex::Index(n) => start = n,
            };

            lines -= 1;
        }

        start
    }

    // Counts how many visible lines/items (depending on mode) there are between start and end.
    //
    // start is counted as visible, and end is not counted as visible.
    //
    // If start == end, we return 0.
    //
    // We won't count more than max lines past start. If we still haven't gotten to end,
    // we'll return max.
    fn count_visible_rows_before(&self, mut start: Index, end: Index, max: u16, mode: Mode) -> u16 {
        let mut num_visible: u16 = 0;
        while start < end && num_visible < max {
            num_visible += 1;
            start = match mode {
                Mode::Line => self.flatjson.next_visible_row(start).unwrap(),
                Mode::Data => self.flatjson.next_item(start).unwrap(),
            };
        }
        num_visible
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flatjson::parse_top_level_json;

    const OBJECT: &'static str = r#"{
        "1": 1,
        "2": [
            3,
            "4"
        ],
        "6": {
            "7": null,
            "8": true,
            "9": 9
        },
        "11": 11
    }"#;

    // Same object as DATA, but formatted as it would appear in data mode
    const DATA_OBJECT: &'static str = r#"{
        "1": 1,
        "2": [
            3,
            "4"],
        "6": {
            "7": null,
            "8": true,
            "9": 9},
        "11": 11}"#;

    #[test]
    fn test_move_up_down_line_mode() {
        let fj = parse_top_level_json(OBJECT.to_owned()).unwrap();
        let mut viewer = JsonViewer::new(fj, Mode::Line);

        assert_movements(
            &mut viewer,
            vec![(Action::MoveDown(1), 1), (Action::MoveDown(2), 3)],
        );

        viewer.flatjson.collapse(6);
        viewer.focused_row = 6;

        assert_movements(
            &mut viewer,
            vec![
                (Action::MoveDown(1), 11),
                (Action::MoveDown(1), 12),
                (Action::MoveDown(1), 12),
            ],
        );

        assert_movements(
            &mut viewer,
            vec![
                (Action::MoveUp(2), 6),
                (Action::MoveUp(1), 5),
                (Action::MoveUp(5), 0),
                (Action::MoveUp(2), 0),
            ],
        );

        viewer.flatjson.collapse(0);
        assert_movements(
            &mut viewer,
            vec![(Action::MoveUp(1), 0), (Action::MoveDown(1), 0)],
        );
    }

    #[test]
    fn test_move_up_down_data_mode() {
        let fj = parse_top_level_json(DATA_OBJECT.to_owned()).unwrap();
        let mut viewer = JsonViewer::new(fj, Mode::Data);

        assert_movements(
            &mut viewer,
            vec![
                (Action::MoveDown(1), 1),
                (Action::MoveDown(3), 4),
                (Action::MoveDown(1), 6),
            ],
        );

        viewer.flatjson.collapse(6);

        assert_movements(
            &mut viewer,
            vec![(Action::MoveDown(1), 11), (Action::MoveDown(1), 11)],
        );

        assert_movements(
            &mut viewer,
            vec![
                (Action::MoveUp(1), 6),
                (Action::MoveUp(3), 2),
                (Action::MoveUp(1), 1),
                (Action::MoveUp(1), 0),
                (Action::MoveUp(1), 0),
            ],
        );
    }

    #[test]
    fn test_move_left_right_line_mode() {
        let fj = parse_top_level_json(OBJECT.to_owned()).unwrap();
        let mut viewer = JsonViewer::new(fj, Mode::Line);

        assert_movements(
            &mut viewer,
            vec![
                (Action::MoveRight, 1),
                (Action::MoveRight, 1),
                (Action::MoveDown(1), 2),
                (Action::MoveRight, 3),
                (Action::MoveLeft, 2),
                (Action::MoveLeft, 2),
            ],
        );

        assert!(viewer.flatjson[2].is_collapsed());

        viewer.focused_row = 10;
        assert_movements(
            &mut viewer,
            vec![
                // Right on closing brace takes you to previous line
                (Action::MoveRight, 9),
                (Action::MoveLeft, 6),
                (Action::MoveDown(4), 10),
                // Collapsing while on closing brace takes you to opening brace
                (Action::MoveLeft, 6),
            ],
        );

        assert!(viewer.flatjson[6].is_collapsed());

        assert_movements(
            &mut viewer,
            vec![
                (Action::MoveLeft, 0),
                (Action::MoveLeft, 0),
                (Action::MoveDown(1), 0),
            ],
        );

        assert!(viewer.flatjson[0].is_collapsed());
        assert_movements(&mut viewer, vec![(Action::MoveRight, 0)]);

        assert!(viewer.flatjson[0].is_expanded());
        assert_movements(&mut viewer, vec![(Action::MoveRight, 1)]);
    }

    #[test]
    fn test_move_left_right_data_mode() {
        let fj = parse_top_level_json(DATA_OBJECT.to_owned()).unwrap();
        let mut viewer = JsonViewer::new(fj, Mode::Data);

        assert_movements(
            &mut viewer,
            vec![
                (Action::MoveRight, 1),
                (Action::MoveRight, 1),
                (Action::MoveDown(5), 7),
                (Action::MoveLeft, 6),
                (Action::MoveLeft, 6),
            ],
        );

        assert!(viewer.flatjson[6].is_collapsed());

        assert_movements(
            &mut viewer,
            vec![
                (Action::MoveLeft, 0),
                (Action::MoveRight, 1),
                (Action::MoveLeft, 0),
                (Action::MoveLeft, 0),
            ],
        );

        assert!(viewer.flatjson[0].is_collapsed());
        assert_movements(
            &mut viewer,
            vec![(Action::MoveDown(1), 0), (Action::MoveRight, 0)],
        );

        assert!(viewer.flatjson[0].is_expanded());
        assert_movements(&mut viewer, vec![(Action::MoveLeft, 0)]);
    }

    fn assert_movements(viewer: &mut JsonViewer, actions_and_focuses: Vec<(Action, Index)>) {
        for (i, (action, expected_focused_row)) in actions_and_focuses.into_iter().enumerate() {
            viewer.perform_action(action);
            assert_eq!(
                viewer.focused_row, expected_focused_row,
                "expected row {} to be focused after {} actions (last action: {:?})",
                expected_focused_row, i, action,
            );
        }
    }

    #[test]
    fn test_ensure_focused_line_is_visible_in_line_mode() {
        let fj = parse_top_level_json(OBJECT.to_owned()).unwrap();
        let mut viewer = JsonViewer::new(fj, Mode::Line);
        viewer.height = 8;
        viewer.scrolloff_setting = 2;

        viewer.ensure_focused_row_is_visible();
        assert_eq!(viewer.top_row, 0);

        // Test pushing past bottom
        assert_window_tracking(
            &mut viewer,
            vec![
                (Action::MoveDown(1), 0, 1),
                (Action::MoveDown(5), 1, 6),
                (Action::MoveDown(1), 2, 7),
            ],
        );

        // Test pushing past top
        assert_window_tracking(
            &mut viewer,
            vec![
                (Action::MoveUp(1), 2, 6),
                (Action::MoveUp(3), 1, 3),
                (Action::MoveUp(1), 0, 2),
                // Top is now top of file
                (Action::MoveUp(1), 0, 1),
            ],
        );

        // Test pushing past bottom at end of file
        assert_window_tracking(
            &mut viewer,
            vec![
                // Move to bottom of file
                (Action::MoveDown(9), 5, 10),
                // Push past bottom
                (Action::MoveDown(1), 5, 11),
                (Action::MoveDown(1), 5, 12),
            ],
        );

        // Put bottom of file on top of screen
        viewer.top_row = 8;
        viewer.focused_row = 10;

        assert_window_tracking(
            &mut viewer,
            vec![
                (Action::MoveDown(1), 8, 11),
                (Action::MoveDown(1), 8, 12),
                (Action::MoveUp(2), 8, 10),
                (Action::MoveUp(1), 7, 9),
            ],
        );

        viewer.top_row = 0;
        viewer.focused_row = 0;
        viewer.height = 6;
        viewer.flatjson.collapse(2);

        // Test with collapsed items
        assert_window_tracking(
            &mut viewer,
            vec![
                (Action::MoveDown(3), 0, 6),
                (Action::MoveDown(1), 1, 7),
                (Action::MoveDown(1), 2, 8),
                (Action::MoveDown(1), 6, 9),
                // Back up now
                (Action::MoveUp(2), 2, 7),
                (Action::MoveUp(1), 1, 6),
                (Action::MoveUp(1), 0, 2),
            ],
        );
    }

    #[test]
    fn test_ensure_focused_line_is_visible_in_data_mode() {
        let fj = parse_top_level_json(DATA_OBJECT.to_owned()).unwrap();
        let mut viewer = JsonViewer::new(fj, Mode::Data);
        viewer.height = 7;
        viewer.scrolloff_setting = 2;

        viewer.ensure_focused_row_is_visible();
        assert_eq!(viewer.top_row, 0);

        // Test pushing past bottom
        assert_window_tracking(
            &mut viewer,
            vec![
                (Action::MoveDown(1), 0, 1),
                (Action::MoveDown(4), 1, 6),
                (Action::MoveDown(1), 2, 7),
            ],
        );

        // Test pushing past top
        assert_window_tracking(
            &mut viewer,
            vec![
                (Action::MoveUp(1), 2, 6),
                (Action::MoveUp(2), 1, 3),
                (Action::MoveUp(1), 0, 2),
                // Top is now top of file
                (Action::MoveUp(1), 0, 1),
            ],
        );

        // Test pushing past bottom at end of file
        assert_window_tracking(
            &mut viewer,
            vec![
                // Move to bottom of file
                (Action::MoveDown(6), 3, 8),
                // Push past bottom
                (Action::MoveDown(1), 3, 9),
                (Action::MoveDown(1), 3, 11),
            ],
        );

        // Put bottom of file on top of screen
        viewer.top_row = 6;
        viewer.focused_row = 8;

        assert_window_tracking(
            &mut viewer,
            vec![
                (Action::MoveDown(1), 6, 9),
                (Action::MoveDown(1), 6, 11),
                (Action::MoveUp(2), 6, 8),
                (Action::MoveUp(1), 4, 7),
            ],
        );

        viewer.top_row = 0;
        viewer.focused_row = 0;
        viewer.height = 5;
        viewer.flatjson.collapse(2);

        // Test with collapsed items
        assert_window_tracking(
            &mut viewer,
            vec![
                (Action::MoveDown(2), 0, 2),
                (Action::MoveDown(1), 1, 6),
                (Action::MoveDown(1), 2, 7),
                (Action::MoveDown(1), 6, 8),
                // Back up now
                (Action::MoveUp(1), 2, 7),
                (Action::MoveUp(1), 1, 6),
                (Action::MoveUp(1), 0, 2),
            ],
        );
    }

    #[test]
    fn test_scroll() {
        let fj = parse_top_level_json(OBJECT.to_owned()).unwrap();
        let mut viewer = JsonViewer::new(fj, Mode::Line);
        viewer.height = 8;
        viewer.scrolloff_setting = 2;

        assert_window_tracking(
            &mut viewer,
            vec![
                (Action::ScrollDown(1), 1, 3),
                (Action::ScrollDown(1), 2, 4),
                (Action::ScrollDown(3), 5, 7),
                // Can scroll so end of file is in middle of screen
                (Action::ScrollDown(1), 6, 8),
                (Action::ScrollDown(4), 10, 12),
                // Can scoll past scrolloff padding
                (Action::ScrollDown(1), 11, 12),
                (Action::ScrollDown(1), 12, 12),
                // Can't scroll past last line
                (Action::ScrollDown(1), 12, 12),
                // Can scroll one up
                (Action::ScrollUp(1), 11, 12),
                (Action::ScrollDown(1), 12, 12),
                // But moving up activates scrolloff
                (Action::MoveUp(1), 9, 11),
            ],
        );

        viewer.top_row = 12;
        viewer.focused_row = 12;

        assert_window_tracking(
            &mut viewer,
            vec![
                (Action::ScrollUp(1), 11, 12),
                (Action::ScrollUp(1), 10, 12),
                (Action::ScrollUp(4), 6, 11),
                (Action::ScrollUp(1), 5, 10),
                // Can't scroll up past top of file
                (Action::ScrollUp(6), 0, 5),
            ],
        );
    }

    fn assert_window_tracking(
        viewer: &mut JsonViewer,
        actions_and_rows: Vec<(Action, usize, usize)>,
    ) {
        for (i, (action, top_row, focused_row)) in actions_and_rows.into_iter().enumerate() {
            viewer.perform_action(action);
            assert_eq!(
                viewer.focused_row,
                focused_row,
                "Incorrect focused_row after {} actions",
                i + 1
            );
            assert_eq!(
                viewer.top_row,
                top_row,
                "Incorrect top_row after {} actions",
                i + 1
            );
        }
    }
}
