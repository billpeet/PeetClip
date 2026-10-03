//! The visible UI is entirely WinUI 3, reconciled by Windows Reactor.
use crate::{config::Config, win};
use windows_reactor::*;

#[derive(Clone)]
pub struct Item {
    pub preview: String,
}

pub enum Message {
    History(Vec<Item>, String),
    Opened,
    Move(i32),
    Copy(usize),
    Hide,
    Exit,
    Settings,
    ShortcutChanged(String),
    LimitChanged(String),
    SaveSettings,
    CancelSettings,
    UpdatesChanged,
}

struct SettingsDraft {
    shortcut: String,
    limit: String,
    error: String,
}

pub struct ClipboardView {
    items: Vec<Item>,
    references: Vec<ElementRef<Button>>,
    selected: usize,
    focus_revision: u64,
    hotkey: String,
    settings: Option<SettingsDraft>,
}

impl Component for ClipboardView {
    type Input = ();
    type Message = Message;

    fn create(_: &(), context: &ComponentContext<Self>) -> Self {
        win::connect(context.sender());
        Self {
            items: Vec::new(),
            references: Vec::new(),
            selected: 0,
            focus_revision: 0,
            hotkey: String::new(),
            settings: None,
        }
    }

    fn update(&mut self, message: Message, context: &ComponentContext<Self>) {
        match message {
            Message::UpdatesChanged => (),
            Message::History(items, hotkey) => {
                self.references.resize_with(items.len(), ElementRef::new);
                self.items = items;
                self.hotkey = hotkey;
                self.selected = self.selected.min(self.items.len().saturating_sub(1));
            }
            Message::Opened => {
                self.selected = 0;
                self.focus_revision += 1;
            }
            Message::Move(delta) => {
                self.selected = (self.selected as i32 + delta)
                    .clamp(0, self.items.len().saturating_sub(1) as i32)
                    as usize;
                self.focus_revision += 1;
            }
            Message::Copy(index) => win::copy(index),
            Message::Hide => win::hide(),
            Message::Exit => {
                win::allow_exit();
                let _ = context.window().request_close();
            }
            Message::Settings => {
                if self.settings.is_none() {
                    let config = win::begin_settings();
                    self.settings = Some(SettingsDraft {
                        shortcut: config.hotkey,
                        limit: config.history_limit.to_string(),
                        error: String::new(),
                    });
                }
            }
            Message::ShortcutChanged(value) => {
                if let Some(draft) = &mut self.settings
                    && draft.shortcut != value
                {
                    draft.shortcut = value;
                    draft.error.clear();
                }
            }
            Message::LimitChanged(value) => {
                if let Some(draft) = &mut self.settings
                    && draft.limit != value
                {
                    draft.limit = value;
                    draft.error.clear();
                }
            }
            Message::SaveSettings => {
                if let Some(draft) = &mut self.settings {
                    match Config::from_fields(&draft.shortcut, &draft.limit)
                        .and_then(win::save_settings)
                    {
                        Ok(()) => {
                            self.settings = None;
                            win::end_settings();
                            self.focus_revision += 1;
                        }
                        Err(error) => draft.error = error,
                    }
                }
            }
            Message::CancelSettings => {
                self.settings = None;
                win::end_settings();
                self.focus_revision += 1;
            }
        }
        #[cfg(feature = "ui-smoke-test")]
        {
            crate::win::smoke::selected(self.selected);
            crate::win::smoke::settings_status(
                self.settings
                    .as_ref()
                    .is_some_and(|draft| !draft.error.is_empty()),
            );
        }
    }

    fn view(&self, _: &(), context: &mut ViewContext<Self>) -> View {
        context.window_title("PeetClip");
        context.window_visuals(
            WindowVisuals::new()
                .client_size(400.0, 420.0)
                .theme(WindowTheme::System)
                .backdrop(WindowBackdrop::Mica)
                .constraints(WindowConstraints {
                    min_width: Some(340.0),
                    min_height: Some(220.0),
                    ..Default::default()
                }),
        );
        context.use_effect("attach-native-services", (), || {
            win::attach();
            None
        });
        if self.settings.is_none()
            && let Some(reference) = self.references.get(self.selected).cloned()
        {
            context.use_effect("focus-selected", self.focus_revision, move || {
                let _ = reference.request_focus();
                None
            });
        }

        let title = Grid::new()
            .columns([GridLength::Auto, GridLength::Star(1.0), GridLength::Auto])
            .margin(Thickness::new(2.0, 0.0, 2.0, 8.0))
            .children((
                Border::new()
                    .width(24.0)
                    .height(24.0)
                    .corner_radius(5.0)
                    .background(ThemeBrush::Accent)
                    .margin(Thickness::new(0.0, 0.0, 8.0, 0.0))
                    .content(FontIcon::new().glyph("\u{e77f}")),
                TextBlock::new()
                    .grid_column(1)
                    .vertical_alignment(VerticalAlignment::Center)
                    .text("PeetClip")
                    .font_size(14.0)
                    .font_weight(FontWeight::SEMI_BOLD),
                TextBlock::new()
                    .grid_column(2)
                    .vertical_alignment(VerticalAlignment::Center)
                    .text(format!("{} items · {}", self.items.len(), self.hotkey))
                    .font_size(11.0)
                    .opacity(0.65),
            ));

        let rows: Vec<View> = self
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                Button::new()
                    .element_ref(&self.references[index])
                    .style(if index == self.selected {
                        ButtonStyle::Accent
                    } else {
                        ButtonStyle::Default
                    })
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .horizontal_content_alignment(HorizontalAlignment::Stretch)
                    .min_height(32.0)
                    .on_click(context.callback(move |()| Message::Copy(index)))
                    .content(
                        Grid::new()
                            .columns([GridLength::Pixel(28.0), GridLength::Star(1.0)])
                            .children((
                                TextBlock::new()
                                    .text((index + 1).to_string())
                                    .font_size(12.0)
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .vertical_alignment(VerticalAlignment::Center),
                                TextBlock::new()
                                    .grid_column(1)
                                    .vertical_alignment(VerticalAlignment::Center)
                                    .text(&item.preview)
                                    .font_size(13.0)
                                    .text_trimming(TextTrimming::CharacterEllipsis),
                            )),
                    )
            })
            .collect();

        let body = if rows.is_empty() {
            StackPanel::new()
                .spacing(4.0)
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    TextBlock::new()
                        .text("Your clipboard, within reach.")
                        .font_size(15.0)
                        .font_weight(FontWeight::SEMI_BOLD),
                    TextBlock::new()
                        .text("Copy some text to start your history.")
                        .opacity(0.65),
                ))
        } else {
            ScrollViewer::new()
                .vertical_scroll_bar_visibility(ScrollBarVisibility::Auto)
                .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
                .content(
                    StackPanel::new().spacing(2.0).keyed_children(
                        rows.into_iter()
                            .enumerate()
                            .map(|(i, row)| KeyedView::new(i as u64, row)),
                    ),
                )
        };

        let popup = Border::new()
            .padding(8.0)
            .background(ThemeBrush::SolidBackground)
            .content(
                Grid::new()
                    .rows([GridLength::Auto, GridLength::Star(1.0), GridLength::Auto])
                    .children((
                        title,
                        Border::new().grid_row(1).content(body),
                        Grid::new()
                            .grid_row(2)
                            .columns([GridLength::Star(1.0), GridLength::Auto])
                            .margin(Thickness::new(2.0, 8.0, 0.0, 0.0))
                            .children((
                                TextBlock::new()
                                    .text("↑↓ Select · Enter Copy · 1–9 Copy · Esc Hide")
                                    .font_size(10.0)
                                    .vertical_alignment(VerticalAlignment::Center)
                                    .opacity(0.65),
                                Button::new()
                                    .grid_column(1)
                                    .style(ButtonStyle::TextLink)
                                    .on_click(context.callback(|()| Message::Settings))
                                    .content(TextBlock::new().text("Settings").font_size(11.0)),
                            )),
                    )),
            );

        let dialog = ContentDialog::new().title("Settings").is_open(self.settings.is_some())
            .on_closed(context.callback(|_| Message::CancelSettings))
            .content(if let Some(draft) = &self.settings {
                ScrollViewer::new().max_height(300.0)
                    .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
                    .vertical_scroll_bar_visibility(ScrollBarVisibility::Auto)
                    .content(StackPanel::new().spacing(8.0).children((
                        TextBlock::new().text("Keyboard shortcut").font_size(12.0),
                        TextBox::new().text(&draft.shortcut).placeholder_text("Alt+C")
                            .on_text_changed(context.callback(Message::ShortcutChanged)),
                        TextBlock::new().text("Win, Ctrl, Alt, Shift + A–Z, 0–9 or F1–F11.\nWin+C falls back to Alt+C if unavailable.")
                            .font_size(11.0).text_wrapping(TextWrapping::Wrap).opacity(0.7),
                        TextBlock::new().text("History limit (1–200 items)").font_size(12.0),
                        TextBox::new().text(&draft.limit).on_text_changed(context.callback(Message::LimitChanged)),
                        TextBlock::new().text(if draft.error.is_empty() { "Reducing the limit removes the oldest entries." } else { &draft.error })
                            .font_size(11.0).text_wrapping(TextWrapping::Wrap)
                            .foreground(if draft.error.is_empty() { ThemeBrush::PrimaryText } else { ThemeBrush::SystemCritical }),
                        StackPanel::new().orientation(Orientation::Horizontal).spacing(8.0).children((
                            Button::new().style(ButtonStyle::Accent).on_click(context.callback(|()| Message::SaveSettings)).content("Save"),
                            Button::new().on_click(context.callback(|()| Message::CancelSettings)).content("Cancel"),
                        )),
                        TextBlock::new().text(crate::updates::status()).font_size(11.0).text_wrapping(TextWrapping::Wrap).opacity(0.7),
                    )))
            } else { View::empty() });
        Grid::new().children((popup, dialog))
    }
}
