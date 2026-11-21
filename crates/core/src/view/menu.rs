use std::thread;
use crate::device::CURRENT_DEVICE;
use crate::font::{Fonts, font_from_style, NORMAL_STYLE};
use crate::geom::{Point, Rectangle, CornerSpec, BorderSpec, small_half, big_half};
use crate::gesture::GestureEvent;
use crate::unit::scale_by_dpi;
use crate::color::{BLACK, WHITE, SEPARATOR_NORMAL, SEPARATOR_STRONG};
use crate::framebuffer::{Framebuffer, UpdateMode};
use super::filler::Filler;
use super::menu_entry::MenuEntry;
use super::common::locate_by_id;
use super::{View, Event, Hub, Bus, RenderQueue, RenderData};
use super::{EntryKind, ViewId, Id, ID_FEEDER, CLOSE_IGNITION_DELAY};
use super::{THICKNESS_MEDIUM, THICKNESS_LARGE, BORDER_RADIUS_MEDIUM};
use crate::context::Context;

pub struct Menu {
    id: Id,
    rect: Rectangle,
    children: Vec<Box<dyn View>>,
    view_id: ViewId,
    kind: MenuKind,
    center: Point,
    root: bool,
    sub_id: u8,
    dir: i32,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum MenuKind {
    DropDown,
    SubMenu,
    Contextual,
}

// TOP MENU       C
//    ───         B
//  ↓  A       ↑  A            
//     B         ───
//     C     BOTTOM MENU

struct MenuColumn {
    entries: Vec<EntryKind>,
    count: usize,
    width: i32,
}

fn menu_columns(text_widths: &[i32], entries: &[EntryKind], max_rows: usize,
                max_width: i32, padding: i32, border: i32, divider: i32,
                more_width: i32) -> Vec<MenuColumn> {
    let mut columns = Vec::new();
    let mut start = 0;
    let mut used_width = 2 * border;
    while start < entries.len() {
        // A separator on a column boundary would draw against the border.
        while entries.get(start).is_some_and(EntryKind::is_separator) {
            start += 1;
        }
        if start == entries.len() {
            break;
        }
        let gap = if columns.is_empty() { 0 } else { divider };
        let remaining = max_width - used_width - gap;
        if !columns.is_empty() && remaining < padding + more_width {
            append_overflow(&mut columns, &entries[start..]);
            break;
        }

        let mut count = 0;
        let mut end = start;
        let mut text_width = 0;
        while end < entries.len() {
            if !entries[end].is_separator() {
                if count == max_rows {
                    break;
                }
                let next_width = text_width.max(text_widths[end]);
                if !columns.is_empty() && padding + next_width > remaining {
                    break;
                }
                count += 1;
                text_width = next_width;
            }
            end += 1;
        }

        if count == 0 {
            append_overflow(&mut columns, &entries[start..]);
            break;
        }
        let mut column_entries = entries[start..end].to_vec();
        while column_entries.last().is_some_and(EntryKind::is_separator) {
            column_entries.pop();
        }
        let width = (padding + text_width.max(more_width)).min(remaining);
        columns.push(MenuColumn { entries: column_entries, count, width });
        used_width += gap + width;
        start = end;
    }
    columns
}

fn available_rows(anchor: i32, dir: i32, screen_height: i32, edge_padding: i32,
                  border_space: i32, entry_height: i32) -> usize {
    let space = if dir.is_positive() {
        screen_height - edge_padding - anchor
    } else {
        anchor - edge_padding
    };
    ((space - border_space) / entry_height).max(0) as usize
}

fn append_overflow(columns: &mut [MenuColumn], remaining: &[EntryKind]) {
    let last = columns.last_mut().unwrap();
    let mut overflow = vec![last.entries.pop().unwrap()];
    overflow.extend_from_slice(remaining);
    while last.entries.last().is_some_and(EntryKind::is_separator) {
        last.entries.pop();
    }
    last.entries.push(EntryKind::More(overflow));
}

impl Menu {
    pub fn new(target: Rectangle, view_id: ViewId, kind: MenuKind, entries: Vec<EntryKind>, context: &mut Context) -> Menu {
        let id = ID_FEEDER.next();
        let mut children = Vec::new();
        let dpi = CURRENT_DEVICE.dpi;
        let (width, height) = context.display.dims;

        let thickness = scale_by_dpi(THICKNESS_MEDIUM, dpi) as i32;
        let border_thickness = scale_by_dpi(THICKNESS_LARGE, dpi) as i32;
        let border_radius = scale_by_dpi(BORDER_RADIUS_MEDIUM - THICKNESS_LARGE, dpi) as i32;

        let sep_color = if context.fb.monochrome() { SEPARATOR_STRONG } else { SEPARATOR_NORMAL };
        let font = font_from_style(&mut context.fonts, &NORMAL_STYLE, dpi);
        let entry_height = font.x_heights.0 as i32 * 5;
        let padding = 4 * font.em() as i32;

        let north_space = target.min.y;
        let south_space = height as i32 - target.max.y;
        let center = target.center();

        let (dir, y_start): (i32, i32) = if kind == MenuKind::SubMenu {
            if north_space < south_space {
                (1, target.min.y - border_thickness)
            } else {
                (-1, target.max.y + border_thickness)
            }
        } else {
            if north_space < south_space {
                (1, target.max.y)
            } else {
                (-1, target.min.y)
            }
        };

        // Menus can use the full display, leaving only their rounded-corner
        // clearance at either edge. Reserving a menu bar here hides usable rows.
        let edge_padding = border_radius + border_thickness;

        let border_space = if kind == MenuKind::DropDown {
            border_thickness
        } else {
            2 * border_thickness
        };

        let rows_available = available_rows(y_start, dir, height as i32, edge_padding, border_space, entry_height);
        let total_entries = entries.iter().filter(|entry| !entry.is_separator()).count();

        // A More submenu must have room for an item and another More row,
        // even when the anchor is near the middle of a short display.
        let max_rows = rows_available.max(2);
        let menu_height = total_entries.clamp(1, max_rows) as i32 * entry_height + border_space;
        let y_start = if rows_available < 2 {
            if dir.is_positive() {
                y_start.min(height as i32 - menu_height).max(0)
            } else {
                y_start.max(menu_height).min(height as i32)
            }
        } else {
            y_start
        };

        let max_width = 9 * width as i32 / 10;
        let divider = thickness;
        let more_width = font.plan("More", None, None).width;
        let widths: Vec<_> = entries.iter()
            .map(|entry| font.plan(entry.text(), None, None).width)
            .collect();
        let columns = menu_columns(&widths, &entries, max_rows, max_width, padding,
                                border_thickness, divider, more_width);

        let menu_width = (columns.iter().map(|column| column.width).sum::<i32>()
            + divider * columns.len().saturating_sub(1) as i32 + 2 * border_thickness)
            .max((padding + more_width + 2 * border_thickness).min(max_width));
        let (mut x_min, mut x_max) = if kind == MenuKind::SubMenu {
            let west_space = target.min.x;
            let east_space = width as i32 - target.max.x;
            if west_space > east_space {
                (target.min.x - menu_width, target.min.x)
            } else {
                (target.max.x, target.max.x + menu_width)
            }
        } else {
            (center.x - small_half(menu_width), center.x + big_half(menu_width))
        };

        if x_min < 0 {
            x_max -= x_min;
            x_min = 0;
        }

        if x_max > width as i32 {
            x_min += width as i32 - x_max;
            x_max = width as i32;
        }

        let triangle_space = if kind == MenuKind::Contextual {
            font.x_heights.1 as i32
        } else {
            0
        };

        let menu_rows = columns.iter().map(|column| column.count).max().unwrap_or(0);
        let menu_height = menu_rows as i32 * entry_height + border_space;
        let mut left = x_min + border_thickness;
        for (column_index, column) in columns.iter().enumerate() {
            let right = left + column.width;
            let mut y_pos = y_start + dir * (border_space - border_thickness);
            let mut row_number = 0;

            for (i, entry) in column.entries.iter().enumerate() {
                if entry.is_separator() {
                    let rect = rect![left, y_pos - small_half(thickness),
                                     right, y_pos + big_half(thickness)];
                    children.push(Box::new(Filler::new(rect, sep_color)) as Box<dyn View>);
                    continue;
                }

                let (y_min, y_max) = if dir.is_positive() {
                    (y_pos, y_pos + entry_height)
                } else {
                    (y_pos - entry_height, y_pos)
                };
                let anchor = rect![left, y_min, right, y_max];
                let mut rect = anchor;

                if i > 0 && column.entries[i - 1].is_separator() {
                    if dir.is_positive() {
                        rect.min.y += big_half(thickness);
                    } else {
                        rect.max.y -= small_half(thickness);
                    }
                }
                if column.entries.get(i + 1).is_some_and(EntryKind::is_separator) {
                    if dir.is_positive() {
                        rect.max.y -= small_half(thickness);
                    } else {
                        rect.min.y += big_half(thickness);
                    }
                }

                let near_edge = row_number == 0 && kind != MenuKind::DropDown;
                let far_edge = row_number + 1 == menu_rows;
                let (top, bottom) = if dir.is_positive() {
                    (near_edge, far_edge)
                } else {
                    (far_edge, near_edge)
                };
                let west = column_index == 0;
                let east = column_index + 1 == columns.len();
                let radius = |rounded| if rounded { border_radius } else { 0 };
                let corner_spec = ((west || east) && (top || bottom)).then(|| CornerSpec::Detailed {
                    north_west: radius(top && west),
                    north_east: radius(top && east),
                    south_east: radius(bottom && east),
                    south_west: radius(bottom && west),
                });

                children.push(Box::new(MenuEntry::new(rect, entry.clone(), anchor, corner_spec)) as Box<dyn View>);
                row_number += 1;
                y_pos += dir * entry_height;
            }

            if column_index + 1 < columns.len() {
                let near = y_start + dir * (border_space - border_thickness);
                let far = near + dir * menu_rows as i32 * entry_height;
                let rect = rect![right, near.min(far), right + divider, near.max(far)];
                children.push(Box::new(Filler::new(rect, sep_color)) as Box<dyn View>);
            }
            left = right + divider;
        }

        let (y_min, y_max) = if dir.is_positive() {
            (y_start - triangle_space, y_start + menu_height)
        } else {
            (y_start - menu_height, y_start + triangle_space)
        };

        let rect = rect![x_min, y_min,
                         x_max, y_max];

        Menu {
            id,
            rect,
            children,
            view_id,
            kind,
            center,
            root: true,
            sub_id: 0,
            dir,
        }
    }

    pub fn root(mut self, root: bool) -> Menu {
        self.root = root;
        self
    }
}

impl View for Menu {
    fn handle_event(&mut self, evt: &Event, hub: &Hub, bus: &mut Bus, rq: &mut RenderQueue, context: &mut Context) -> bool {
        match *evt {
            Event::Select(ref entry_id) if self.root => {
                self.handle_event(&Event::PropagateSelect(entry_id.clone()), hub, bus, rq, context);
                false
            },
            Event::PropagateSelect(..) => {
                for c in &mut self.children {
                    if c.handle_event(evt, hub, bus, rq, context) {
                        break;
                    }
                }
                true
            },
            Event::Validate if self.root => {
                let hub2 = hub.clone();
                let view_id = self.view_id;
                thread::spawn(move || {
                    thread::sleep(CLOSE_IGNITION_DELAY);
                    hub2.send(Event::Close(view_id)).ok();
                });
                true
            },
            Event::Gesture(GestureEvent::Tap(center)) if !self.rect.includes(center) => {
                if self.root {
                    bus.push_back(Event::Close(self.view_id));
                } else {
                    bus.push_back(Event::CloseSub(self.view_id));
                }
                self.root
            },
            Event::Gesture(GestureEvent::HoldFingerShort(center, ..)) if !self.rect.includes(center) => self.root,
            Event::SubMenu(rect, ref entries, kind) => {
                let menu = Menu::new(rect, ViewId::SubMenu(self.sub_id),
                                     kind, entries.clone(), context).root(false);
                rq.add(RenderData::new(menu.id(), *menu.rect(), UpdateMode::Gui));
                self.children.push(Box::new(menu) as Box<dyn View>);
                self.sub_id = self.sub_id.wrapping_add(1);
                true
            },
            Event::CloseSub(id) => {
                if let Some(index) = locate_by_id(self, id) {
                    rq.add(RenderData::expose(*self.children[index].rect(), UpdateMode::Gui));
                    self.children.remove(index);
                }
                true
            },
            Event::Gesture(..) => true,
            _ => false,
        }
    }

    fn render(&self, fb: &mut dyn Framebuffer, _rect: Rectangle, fonts: &mut Fonts) {
        let dpi = CURRENT_DEVICE.dpi;
        let border_radius = scale_by_dpi(BORDER_RADIUS_MEDIUM, dpi) as i32;
        let border_thickness = scale_by_dpi(THICKNESS_LARGE, dpi) as u16;

        let corners = if self.kind == MenuKind::DropDown {
            if self.dir.is_positive() {
                CornerSpec::South(border_radius)
            } else {
                CornerSpec::North(border_radius)
            }
        } else {
            CornerSpec::Uniform(border_radius)
        };

        if self.kind == MenuKind::Contextual {
            let font = font_from_style(fonts, &NORMAL_STYLE, dpi);
            let triangle_space = font.x_heights.1 as i32;
            let mut rect = self.rect;

            if self.dir.is_positive() {
                rect.min.y += triangle_space
            } else {
                rect.max.y -= triangle_space
            }

            fb.draw_rounded_rectangle_with_border(&rect,
                                                  &corners,
                                                  &BorderSpec { thickness: border_thickness,
                                                                color: BLACK },
                                                  &WHITE);

            let y_b = if self.dir.is_positive() {
                self.rect.min.y
            } else {
                self.rect.max.y - 1
            };

            let side = triangle_space + border_thickness as i32;
            let x_b = self.center.x.max(rect.min.x + 2 * side)
                                   .min(rect.max.x - 2 * side);

            let mut b = pt!(x_b, y_b);
            let mut a = b + pt!(-side, self.dir * side);
            let mut c = a + pt!(2 * side, 0);

            fb.draw_triangle(&[a, b, c], BLACK);
            let drift = (border_thickness as f32 * ::std::f32::consts::SQRT_2) as i32;

            b += pt!(0, self.dir * drift);
            a += pt!(drift, 0);
            c -= pt!(drift, 0);

            fb.draw_triangle(&[a, b, c], WHITE);
        } else {
            fb.draw_rounded_rectangle_with_border(&self.rect,
                                                  &corners,
                                                  &BorderSpec { thickness: border_thickness,
                                                                color: BLACK },
                                                  &WHITE);
        }
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

    fn view_id(&self) -> Option<ViewId> {
        Some(self.view_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_capacity_uses_space_at_both_screen_edges() {
        // The reported 1072 x 1448 screenshot: five rows fit below y=745.
        assert_eq!(available_rows(745, 1, 1448, 8, 4, 120), 5);
        // Mirror the anchor to check an upward-opening menu near the top.
        assert_eq!(available_rows(703, -1, 1448, 8, 4, 120), 5);
        for dir in [-1, 1] {
            let anchor = if dir > 0 { 836 } else { 612 };
            assert_eq!(available_rows(anchor, dir, 1448, 8, 4, 120), 5);
            assert_eq!(available_rows(anchor + dir, dir, 1448, 8, 4, 120), 4);
        }
        assert_eq!(available_rows(720, 1, 800, 8, 4, 120), 0);
        assert_eq!(available_rows(80, -1, 800, 8, 4, 120), 0);
    }

    #[test]
    fn extra_row_avoids_more_only_when_entries_fit() {
        let entries = vec![EntryKind::Command("Item".into(), super::super::EntryId::SaveTheme); 10];
        let widths = vec![250; entries.len()];
        let columns = menu_columns(&widths, &entries, 5, 900, 150, 2, 2, 90);
        assert_eq!(columns.len(), 2);
        assert!(columns.iter().all(|c| c.count == 5));
        assert!(!columns.iter().flat_map(|c| &c.entries).any(|e| matches!(e, EntryKind::More(_))));
        let columns = menu_columns(&widths, &entries, 4, 900, 150, 2, 2, 90);
        assert!(columns.iter().flat_map(|c| &c.entries).any(|e| matches!(e, EntryKind::More(_))));
    }
}
