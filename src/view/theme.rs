use crate::device::CURRENT_DEVICE;
use crate::framebuffer::{Framebuffer, UpdateMode};
use crate::geom::{Rectangle, CornerSpec, BorderSpec, halves};
use crate::font::{Fonts, font_from_style, NORMAL_STYLE};
use super::{View, Event, Hub, Bus, Id, ID_FEEDER, RenderQueue, RenderData, ViewId, Align};
use super::{SMALL_BAR_HEIGHT, THICKNESS_LARGE, BORDER_RADIUS_MEDIUM};
use super::label::Label;
use super::button::Button;
use super::icon::Icon;
use crate::gesture::GestureEvent;
use crate::color::{BLACK, WHITE};
use crate::unit::scale_by_dpi;
use crate::app::Context;
use std::fmt;
use lazy_static::lazy_static;

const LABEL_SAVE: &str = "Save";
const WIDGET_OFFSET: usize = 2;

#[derive(Debug, Clone)]
pub enum ThemeProp {
    FontFamily = 0,
    FontSize,
    RelativeFontSize,
    MarginWidth,
    // IgnoreDocumentCss,
    LineSpacing,
    TextAlign,
    FrontLight,
    InvertedMode,
    KeepMenuOnScreen,
}

impl fmt::Display for ThemeProp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s: String = format!("{:?}", self)
            .chars().enumerate()
            .map(|(i, c)| if i > 0 && c.is_uppercase() {
                              format!(" {}", c.to_lowercase())
                          } else {
                              c.to_string()
                          }).collect();
        write!(f, "{}", s)
    }
}

lazy_static! {
    static ref THEME_PROPS: Vec<ThemeProp> = vec![
        ThemeProp::FontFamily,
        ThemeProp::FontSize,
        ThemeProp::RelativeFontSize,
        ThemeProp::MarginWidth,
        // ThemeProp::IgnoreDocumentCss,
        ThemeProp::LineSpacing,
        ThemeProp::TextAlign,
        ThemeProp::FrontLight,
        ThemeProp::InvertedMode,
        ThemeProp::KeepMenuOnScreen,
    ];
}

pub struct ThemeDialog {
    id: Id,
    rect: Rectangle,
    children: Vec<Box<dyn View>>,
}

impl ThemeDialog {
    pub fn new(has_relative_fs: bool, context: &mut Context) -> ThemeDialog {
        let id = ID_FEEDER.next();
        let fonts = &mut context.fonts;
        let mut children = Vec::new();
        let dpi = CURRENT_DEVICE.dpi;
        let (width, height) = context.display.dims;
        let window_width = width as i32 * if height > width {9} else {7} / 10;
        let small_height = scale_by_dpi(SMALL_BAR_HEIGHT, dpi) as i32;
        let thickness = scale_by_dpi(THICKNESS_LARGE, dpi) as i32;
        let border_radius = scale_by_dpi(BORDER_RADIUS_MEDIUM, dpi) as i32;

        let (x_height, padding) = {
            let font = font_from_style(fonts, &NORMAL_STYLE, dpi);
            (font.x_heights.0 as i32, font.em() as i32)
        };

        let toggle_height = 4 * x_height;
        let (a, b) = halves(window_width - 3 * padding);
        let toggle_widths = vec![a, b];
        let (a, b) = halves(THEME_PROPS.len() as i32);
        let num_rows = vec![a, b];

        let window_height = a.max(b) * (toggle_height + padding / 2) +  2 * small_height + 5 * padding;

        let dx = (width as i32 - window_width) / 2;
        let dy = (height as i32 - window_height) / 3;

        let rect = rect![dx, dy, dx + window_width, dy + window_height];

        let corners = CornerSpec::Detailed {
            north_west: 0,
            north_east: border_radius - thickness,
            south_east: 0,
            south_west: 0,
        };

        let close_icon = Icon::new("close",
                                   rect![rect.max.x - small_height,
                                         rect.min.y + thickness,
                                         rect.max.x - thickness,
                                         rect.min.y + small_height],
                                   Event::Close(ViewId::ThemeDialog))
                              .corners(Some(corners));

        children.push(Box::new(close_icon) as Box<dyn View>);

        let label = Label::new(rect![rect.min.x + small_height,
                                     rect.min.y + thickness + padding / 2,
                                     rect.max.x - small_height,
                                     rect.min.y + small_height + padding / 2],
                               "Select setting(s) to save in theme".to_string(),
                               Align::Center);

        children.push(Box::new(label) as Box<dyn View>);

        let mut idx = 0;
        let mut y = 0;
        for col in 0..=1 {
            let x = if col == 0 {
                rect.min.x + padding
            } else {
                rect.min.x + 2 * padding + toggle_widths[0]
            };
            y = rect.min.y + small_height + 3 * padding / 2;
            for _ in 0..num_rows[col] {
                let label = THEME_PROPS[idx].to_string();
                let toggle = Button::new(rect![x,
                                               y,
                                               x + toggle_widths[col],
                                               y + toggle_height],
                                          Event::Validate,
                                          label.to_string())
                            .disabled(idx == ThemeProp::RelativeFontSize as usize && !has_relative_fs)
                            .toggle(false);
                children.push(Box::new(toggle) as Box<dyn View>);
                y += toggle_height + padding / 2;
                idx += 1;
            }
        }
        y += 3 * padding / 2;
        let button_width = 10 * x_height;
        let x = rect.max.x - padding - button_width;
        let button_save = Button::new(rect![x,
                                            y,
                                            x + button_width,
                                            y + small_height],
                                      Event::SaveTheme,
                                      LABEL_SAVE.to_string())
                            .disabled(true);
        children.push(Box::new(button_save) as Box<dyn View>);

        ThemeDialog {
            id,
            rect,
            children,
        }
    }

    pub fn is_on(&self, prop: ThemeProp) -> bool {
        self.child(prop as usize + WIDGET_OFFSET).downcast_ref::<Button>().unwrap().toggled()
    }
}

impl View for ThemeDialog {
    fn handle_event(&mut self, evt: &Event, _hub: &Hub, bus: &mut Bus, rq: &mut RenderQueue, _context: &mut Context) -> bool {
        match *evt {
            Event::Gesture(GestureEvent::Tap(center)) if !self.rect.includes(center) => {
                bus.push_back(Event::Close(ViewId::ThemeDialog));
                true
            },
            Event::Gesture(..) => true,
            Event::Validate => {
                let enable_save = self.children.iter().skip(WIDGET_OFFSET).take(THEME_PROPS.len())
                                      .any(|c| c.downcast_ref::<Button>().unwrap().toggled());
                let index = self.len() - 1;
                if let Some(save_button) = self.child_mut(index).downcast_mut::<Button>() {
                    save_button.disabled = !enable_save;
                    rq.add(RenderData::new(save_button.id(), *save_button.rect(), UpdateMode::Gui));
                }
                true
            },
            _ => false,
        }
    }

    fn render(&self, fb: &mut dyn Framebuffer, _rect: Rectangle, _fonts: &mut Fonts) {
        let dpi = CURRENT_DEVICE.dpi;

        let border_radius = scale_by_dpi(BORDER_RADIUS_MEDIUM, dpi) as i32;
        let border_thickness = scale_by_dpi(THICKNESS_LARGE, dpi) as u16;

        fb.draw_rounded_rectangle_with_border(&self.rect,
                                              &CornerSpec::Uniform(border_radius),
                                              &BorderSpec { thickness: border_thickness,
                                                            color: BLACK },
                                              &WHITE);
    }

    fn is_background(&self) -> bool {
        true
    }

    fn rect(&self) -> &Rectangle {
        &self.rect
    }

    fn rect_mut(&mut self) -> &mut Rectangle {
        &mut self.rect
    }

    fn children(&self) -> &Vec<Box<dyn View>> {
        &self.children
    }

    fn children_mut(&mut self) -> &mut Vec<Box<dyn View>> {
        &mut self.children
    }

    fn id(&self) -> Id {
        self.id
    }
}
