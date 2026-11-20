//! Pieces every view shares in the redesign: the page header (title, subtitle and the view's actions at the
//! top right), the list filter with its chips, and the toasts.

use crate::panel::PanelMsg;
use crate::theme::{self, BAD, DIM, HI, LINE, OK};
use crate::Message;
use iced::widget::{button, column, container, row, text, text_input};
use iced::{Background, Color, Element, Length};
use std::time::{Duration, Instant};

/// How long a success toast stays; a refusal stays until dismissed.
pub const TOAST_MS: u64 = 4200;
/// Notices kept for the status bar history.
pub const HISTORY: usize = 30;

thread_local! {
    /// The rows of the list on screen, in order, with what pressing each one sends: Up and Down walk it.
    static NAV: std::cell::RefCell<Vec<(bool, Message)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Forgets the rows of the last frame (called at the start of every panel view).
pub fn nav_reset() {
    NAV.with(|n| n.borrow_mut().clear());
}

/// Wraps what a list row sends when pressed and remembers it, with whether the row is the selected one.
pub fn nav(selected: bool, message: Message) -> Message {
    NAV.with(|n| n.borrow_mut().push((selected, message.clone())));
    message
}

/// What selecting the row `delta` places from the selected one sends (the first row when none is selected).
pub fn nav_step(delta: i32) -> Option<Message> {
    NAV.with(|n| {
        let rows = n.borrow();
        if rows.is_empty() {
            return None;
        }
        let at = rows.iter().position(|(s, _)| *s);
        let next = match at {
            Some(i) => (i as i32 + delta).clamp(0, rows.len() as i32 - 1) as usize,
            None => 0,
        };
        Some(rows[next].1.clone())
    })
}

/// The view's title and subtitle on the left, its actions on the right (`create` first, filled button last).
pub fn page_header<'a>(
    title: &'a str,
    subtitle: String,
    actions: Vec<Element<'a, Message>>,
) -> Element<'a, Message> {
    container(
        row![
            column![
                text(title).font(theme::MONO_BOLD).size(22).color(HI),
                text(subtitle).size(12).color(DIM),
            ]
            .spacing(6),
            container(text("")).width(Length::Fill),
            row(actions).spacing(8).align_y(iced::Alignment::Center),
        ]
        .align_y(iced::Alignment::Center),
    )
    .padding([18, 28])
    .width(Length::Fill)
    .style(theme::bottom_line_style)
    .into()
}

/// The filter box of a list and its chips; `chips` are (label, count) and `active` the picked one.
pub fn filter_bar<'a>(
    placeholder: &'static str,
    value: &str,
    chips: &[(&'static str, usize)],
    active: usize,
) -> Element<'a, Message> {
    let mut chip_row = row![].spacing(6).align_y(iced::Alignment::Center);
    for (i, (label, n)) in chips.iter().enumerate() {
        let text_label = if i == 0 {
            format!("{label} {n}")
        } else {
            (*label).to_owned()
        };
        chip_row = chip_row.push(
            button(text(text_label).size(11.5))
                .padding([3, 10])
                .style(theme::chip_style(i == active))
                .on_press(Message::Panel(PanelMsg::Chip(i))),
        );
    }
    column![
        text_input(placeholder, value)
            .id(FILTER_ID)
            .on_input(|t| Message::Panel(PanelMsg::Filter(t)))
            .style(theme::input_style)
            .padding([8, 10])
            .size(12),
        chip_row.wrap(),
    ]
    .spacing(8)
    .padding(8)
    .into()
}

/// The id of the filter box, so `/` can focus it.
pub const FILTER_ID: &str = "list-filter";

/// What a toast says and whether it is a refusal.
#[derive(Debug, Clone)]
pub struct Toast {
    pub text: String,
    pub error: bool,
    /// Neither success nor refusal: something the user asked for that changed nothing.
    pub info: bool,
    pub at: Instant,
}

impl Toast {
    pub fn new(text: String) -> Self {
        let error = is_error(&text);
        let info = !error && text.starts_with(INFO);
        Self {
            text: text.trim_start_matches(INFO).to_owned(),
            error,
            info,
            at: Instant::now(),
        }
    }

    /// A success toast goes away on its own.
    pub fn expired(&self) -> bool {
        !self.error && self.at.elapsed() > Duration::from_millis(TOAST_MS)
    }
}

/// Marks a notice as information (cyan square): it starts with this character, which is never shown.
pub const INFO: char = '\u{1}';

/// Is this notice a refusal of the app's own form check (see `invalid`)?
pub fn is_invalid(notice: &str) -> bool {
    notice.ends_with('\n')
}

/// An information notice.
pub fn info(message: &str) -> String {
    format!("{INFO}{message}")
}

/// A form that cannot be sent yet: a notice that is a refusal of the app itself (no server involved).
pub fn invalid(message: &str) -> String {
    format!("{message}\n")
}

/// The wording the panel uses for failures and server refusals.
pub fn is_error(notice: &str) -> bool {
    notice.contains('\n')
        || notice.contains("error")
        || notice.contains("failed")
        || notice.contains("✖")
        || notice.contains("refused")
        || notice.contains("cannot")
}

/// The toasts at the bottom right: 400 wide, 14 px from the right edge and 40 px above the status bar, stacked
/// upwards with the newest at the bottom (at most 3). A success goes away by itself; an error stays and shows
/// the server's own words.
pub fn toasts_view<'a>(toasts: Vec<(usize, &'a Toast)>) -> Element<'a, Message> {
    let mut stack = column![].spacing(8).width(400);
    for (index, toast) in toasts {
        let color = if toast.error {
            BAD
        } else if toast.info {
            theme::ACCENT
        } else {
            OK
        };
        let mut body = column![].spacing(6).width(Length::Fill);
        if toast.error {
            // the summary is the first line; what follows is the server's answer, as sent
            let (summary, answer) = toast.text.split_once('\n').unwrap_or(("", &toast.text));
            if !summary.is_empty() {
                body = body.push(text(summary.to_owned()).size(12).color(theme::HI));
            }
            if !answer.is_empty() {
                body = body
                    .push(
                        text("The server refused this. Its answer, as sent:")
                            .size(11)
                            .color(BAD),
                    )
                    .push(
                        container(text(answer.to_owned()).size(11).color(theme::TEXT2))
                            .padding([6, 8])
                            .width(Length::Fill)
                            .style(|_| container::Style {
                                background: Some(Background::Color(theme::INPUT)),
                                border: iced::Border {
                                    color: LINE,
                                    width: 1.0,
                                    radius: 4.0.into(),
                                },
                                ..container::Style::default()
                            }),
                    );
            }
        } else {
            body = body.push(text(toast.text.clone()).size(12).color(theme::FG));
        }
        let close = button(container(text("×").size(14)).center_y(Length::Fill))
            .padding([0, 6])
            .width(22)
            .height(22)
            .style(theme::close_button_style())
            .on_press(Message::Panel(PanelMsg::DismissToast(index)));
        stack = stack.push(
            container(
                row![
                    container(text(""))
                        .width(8)
                        .height(8)
                        .style(move |_| container::Style {
                            background: Some(Background::Color(color)),
                            ..container::Style::default()
                        }),
                    body,
                    close
                ]
                .spacing(10)
                .align_y(iced::Alignment::Start),
            )
            .padding([11, 12])
            .width(400)
            .style(theme::toast_style(Color {
                a: if toast.error { 0.7 } else { 0.4 },
                ..color
            })),
        );
    }
    container(stack)
        .padding(theme::pad(0.0, 14.0, 40.0, 0.0))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Right)
        .align_y(iced::alignment::Vertical::Bottom)
        .into()
}

/// The flexible region of a view: its viewport takes the width that is left, its content keeps at least
/// `min_w`; when the viewport is narrower the region scrolls (both ways) instead of squashing the content.
pub fn flex<'a>(
    min_w: f32,
    pad: [f32; 2],
    build: impl Fn() -> Element<'a, Message> + 'a,
) -> Element<'a, Message> {
    iced::widget::responsive(move |size| {
        let w = (size.width - 8.0).max(min_w);
        let bar = || {
            iced::widget::scrollable::Scrollbar::new()
                .width(8)
                .scroller_width(8)
        };
        theme::scroll(
            container(build())
                .padding([pad[0], pad[1]])
                .width(Length::Fixed(w)),
        )
        .direction(iced::widget::scrollable::Direction::Both {
            vertical: bar(),
            horizontal: bar(),
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    })
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_and_down_walk_the_rows_of_the_frame_and_stop_at_the_ends() {
        let id = |m: Option<Message>| match m {
            Some(Message::Panel(PanelMsg::SelectRun(id))) => id,
            other => panic!("{other:?}"),
        };
        nav_reset();
        assert!(nav_step(1).is_none(), "no list on screen");
        for (i, name) in ["a", "b", "c"].into_iter().enumerate() {
            nav(i == 1, Message::Panel(PanelMsg::SelectRun(name.into())));
        }
        assert_eq!(id(nav_step(1)), "c");
        assert_eq!(id(nav_step(-1)), "a");
        assert_eq!(id(nav_step(-5)), "a");
        nav_reset();
        nav(false, Message::Panel(PanelMsg::SelectRun("x".into())));
        assert_eq!(id(nav_step(1)), "x", "nothing selected: the first row");
    }

    #[test]
    fn an_info_notice_is_neither_a_success_nor_a_refusal_and_hides_its_marker() {
        let toast = Toast::new(info("No idle agents to resume."));
        assert!(toast.info && !toast.error);
        assert_eq!(toast.text, "No idle agents to resume.");
        assert!(Toast::new(invalid("A task needs a title.")).error);
    }
}
