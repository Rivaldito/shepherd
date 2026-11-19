//! Pieces every view shares in the redesign: the page header (title, subtitle and the view's actions at the
//! top right), the list filter with its chips, and the toasts.

use crate::panel::PanelMsg;
use crate::theme::{self, BAD, DIM, FG, HI, LINE, OK};
use crate::Message;
use iced::widget::{button, column, container, row, text, text_input};
use iced::{Background, Element, Length};
use std::time::{Duration, Instant};

/// How long a success toast stays; a refusal stays until dismissed.
pub const TOAST_SECS: u64 = 4;
/// Notices kept for the status bar history.
pub const HISTORY: usize = 50;

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
    pub at: Instant,
}

impl Toast {
    pub fn new(text: String) -> Self {
        let error = is_error(&text);
        Self {
            text,
            error,
            at: Instant::now(),
        }
    }

    /// A success toast goes away on its own.
    pub fn expired(&self) -> bool {
        !self.error && self.at.elapsed() > Duration::from_secs(TOAST_SECS)
    }
}

/// The wording the panel uses for failures and server refusals.
pub fn is_error(notice: &str) -> bool {
    notice.contains("error")
        || notice.contains("failed")
        || notice.contains("✖")
        || notice.contains("refused")
        || notice.contains("cannot")
}

/// The toast at the top right: a green success or a red refusal with the server's own words.
pub fn toast_view<'a>(toast: &'a Toast) -> Element<'a, Message> {
    let color = if toast.error { BAD } else { OK };
    let mut body = column![row![
        text("●").size(10).color(color),
        text(if toast.error {
            "The server refused this. Its answer, as sent:"
        } else {
            ""
        })
        .size(11)
        .color(color),
    ]
    .spacing(8)];
    if !toast.error {
        body = column![row![
            text("●").size(10).color(color),
            text(toast.text.clone()).size(12).color(FG)
        ]
        .spacing(8)];
    } else {
        body = body.push(
            container(text(toast.text.clone()).size(12).color(FG))
                .padding([6, 8])
                .width(Length::Fill)
                .style(|_| container::Style {
                    background: Some(Background::Color(theme::BG)),
                    border: iced::Border {
                        color: LINE,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..container::Style::default()
                }),
        );
    }
    let close = button(text("×").size(14))
        .padding([0, 6])
        .style(theme::close_button_style())
        .on_press(Message::Panel(PanelMsg::DismissToast));
    container(
        container(row![body.width(Length::Fill), close].spacing(8))
            .padding([10, 12])
            .width(420)
            .style(theme::toast_style(color)),
    )
    .padding([56, 16])
    .width(Length::Fill)
    .align_x(iced::alignment::Horizontal::Right)
    .into()
}
