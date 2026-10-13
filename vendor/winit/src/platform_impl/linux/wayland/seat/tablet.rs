//! Native tablet tools share the window pointer route and retain pen pressure.

use crate::dpi::LogicalPosition;
use crate::event::{ElementState, Force, MouseButton, Touch, TouchPhase, WindowEvent};
use crate::platform_impl::wayland::state::WinitState;
use crate::platform_impl::wayland::{self, DeviceId};
use sctk::globals::GlobalData;
use sctk::reexports::client::globals::{BindError, GlobalList};
use sctk::reexports::client::protocol::wl_seat::WlSeat;
use sctk::reexports::client::{event_created_child, Connection, Dispatch, Proxy, QueueHandle};
use sctk::reexports::protocols::wp::tablet::zv2::client::{
    zwp_tablet_manager_v2::ZwpTabletManagerV2,
    zwp_tablet_pad_group_v2::ZwpTabletPadGroupV2,
    zwp_tablet_pad_ring_v2::ZwpTabletPadRingV2,
    zwp_tablet_pad_strip_v2::ZwpTabletPadStripV2,
    zwp_tablet_pad_v2::ZwpTabletPadV2,
    zwp_tablet_seat_v2::ZwpTabletSeatV2,
    zwp_tablet_tool_v2::{self, ZwpTabletToolV2},
    zwp_tablet_v2::ZwpTabletV2,
};
use std::sync::Mutex;

#[derive(Debug)]
pub struct TabletState(ZwpTabletManagerV2);

impl TabletState {
    /// Bind the tablet manager when the compositor advertises it.
    /// Takes the globals and event queue; returns the manager or a binding error.
    pub fn new(globals: &GlobalList, queue: &QueueHandle<WinitState>) -> Result<Self, BindError> {
        globals.bind(queue, 1..=1, GlobalData).map(Self)
    }

    /// Listen for tablets and tools on a compositor seat.
    /// Takes the seat and event queue; returns its tablet protocol handle.
    pub fn seat(&self, seat: &WlSeat, queue: &QueueHandle<WinitState>) -> ZwpTabletSeatV2 {
        self.0.get_tablet_seat(seat, queue, ())
    }
}

#[derive(Debug, Default)]
pub struct ToolData(Mutex<ToolInput>);

#[derive(Debug, Default)]
pub struct PadData(Mutex<Vec<ZwpTabletPadGroupV2>>);

#[derive(Debug, Default)]
pub struct GroupData {
    rings: Mutex<Vec<ZwpTabletPadRingV2>>,
    strips: Mutex<Vec<ZwpTabletPadStripV2>>,
}

#[derive(Debug, Default)]
struct ToolInput {
    window: Option<wayland::WindowId>,
    location: LogicalPosition<f64>,
    pressure: Option<f64>,
    down: bool,
    phase: Option<TouchPhase>,
    leaving: bool,
    buttons: Vec<(ElementState, MouseButton)>,
    held_buttons: Vec<MouseButton>,
}

impl ToolInput {
    /// Keep side-button releases paired with their presses.
    /// Takes a button and state; queues an event only when its held state changes.
    fn button(&mut self, button: MouseButton, state: ElementState) {
        match (
            state,
            self.held_buttons.iter().position(|held| *held == button),
        ) {
            (ElementState::Pressed, None) => {
                self.held_buttons.push(button);
                self.buttons.push((state, button));
            }
            (ElementState::Released, Some(index)) => {
                self.held_buttons.remove(index);
                self.buttons.push((state, button));
            }
            _ => {}
        }
    }

    /// Release held side buttons when a tool leaves proximity.
    /// Takes this tool state and queues the missing releases without duplicates.
    fn release_buttons(&mut self) {
        for button in std::mem::take(&mut self.held_buttons) {
            self.buttons.push((ElementState::Released, button));
        }
    }
}

impl Dispatch<ZwpTabletManagerV2, GlobalData> for WinitState {
    fn event(
        _: &mut Self,
        _: &ZwpTabletManagerV2,
        _: <ZwpTabletManagerV2 as Proxy>::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpTabletSeatV2, ()> for WinitState {
    fn event(
        _: &mut Self,
        _: &ZwpTabletSeatV2,
        _: <ZwpTabletSeatV2 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
    event_created_child!(WinitState, ZwpTabletSeatV2, [0 => (ZwpTabletV2, ()), 1 => (ZwpTabletToolV2, ToolData::default()), 2 => (ZwpTabletPadV2, PadData::default())]);
}

impl Dispatch<ZwpTabletToolV2, ToolData> for WinitState {
    fn event(
        state: &mut Self,
        tool: &ZwpTabletToolV2,
        event: zwp_tablet_tool_v2::Event,
        data: &ToolData,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let mut input = data.0.lock().unwrap();
        match event {
            zwp_tablet_tool_v2::Event::ProximityIn { surface, .. } => {
                input.window = Some(wayland::make_wid(&surface));
                input.leaving = false;
                input.pressure = None;
            }
            zwp_tablet_tool_v2::Event::Motion { x, y } => {
                input.location = LogicalPosition::new(x, y)
            }
            zwp_tablet_tool_v2::Event::Pressure { pressure } => {
                input.pressure = Some(pressure as f64 / 65535.0)
            }
            zwp_tablet_tool_v2::Event::Down { .. } => {
                input.down = true;
                input.phase = Some(TouchPhase::Started);
            }
            zwp_tablet_tool_v2::Event::Up => {
                input.down = false;
                input.phase = Some(TouchPhase::Ended);
            }
            zwp_tablet_tool_v2::Event::ProximityOut => {
                if input.down {
                    input.phase = Some(TouchPhase::Cancelled);
                    input.down = false;
                }
                input.release_buttons();
                input.leaving = true;
            }
            zwp_tablet_tool_v2::Event::Button { button, state, .. } => {
                let button = match button {
                    0x14b => MouseButton::Right,
                    0x14c => MouseButton::Middle,
                    value => MouseButton::Other(value as u16),
                };
                let state = if state
                    == sctk::reexports::client::WEnum::Value(
                        zwp_tablet_tool_v2::ButtonState::Pressed,
                    ) {
                    ElementState::Pressed
                } else {
                    ElementState::Released
                };
                input.button(button, state);
            }
            zwp_tablet_tool_v2::Event::Removed => tool.destroy(),
            zwp_tablet_tool_v2::Event::Frame { .. } => {
                let Some(window_id) = input.window else {
                    return;
                };
                let Some(window) = state.windows.get_mut().get(&window_id) else {
                    return;
                };
                let scale = window.lock().unwrap().scale_factor();
                let device_id =
                    crate::event::DeviceId(crate::platform_impl::DeviceId::Wayland(DeviceId));
                let mut touch = input
                    .phase
                    .take()
                    .or(input.down.then_some(TouchPhase::Moved))
                    .map(|phase| {
                        WindowEvent::Touch(Touch {
                            device_id,
                            phase,
                            location: input.location.to_physical(scale),
                            force: input.pressure.map(Force::Normalized),
                            id: (tool.id().protocol_id() as u64) | (1 << 63),
                        })
                    });
                if !input.leaving {
                    if let Some(event) = touch.take() {
                        state.events_sink.push_window_event(event, window_id);
                    }
                }
                if !input.down && !input.leaving {
                    state.events_sink.push_window_event(
                        WindowEvent::CursorMoved {
                            device_id,
                            position: input.location.to_physical(scale),
                        },
                        window_id,
                    );
                }
                for (button_state, button) in input.buttons.drain(..) {
                    state.events_sink.push_window_event(
                        WindowEvent::MouseInput {
                            device_id,
                            state: button_state,
                            button,
                        },
                        window_id,
                    );
                }
                if let Some(event) = touch {
                    state.events_sink.push_window_event(event, window_id);
                }
                if input.leaving {
                    state
                        .events_sink
                        .push_window_event(WindowEvent::CursorLeft { device_id }, window_id);
                    input.window = None;
                    input.leaving = false;
                }
            }
            _ => {}
        }
    }
}

macro_rules! ignore_tablet_object {
    ($object:ty) => {
        impl Dispatch<$object, ()> for WinitState {
            fn event(
                _: &mut Self,
                proxy: &$object,
                event: <$object as Proxy>::Event,
                _: &(),
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
                let _ = (proxy, event);
            }
        }
    };
}
ignore_tablet_object!(ZwpTabletPadRingV2);
ignore_tablet_object!(ZwpTabletPadStripV2);

impl Dispatch<ZwpTabletV2, ()> for WinitState {
    fn event(
        _: &mut Self,
        tablet: &ZwpTabletV2,
        event: <ZwpTabletV2 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(
            event,
            sctk::reexports::protocols::wp::tablet::zv2::client::zwp_tablet_v2::Event::Removed
        ) {
            tablet.destroy();
        }
    }
}

impl Dispatch<ZwpTabletPadV2, PadData> for WinitState {
    fn event(
        _: &mut Self,
        pad: &ZwpTabletPadV2,
        event: <ZwpTabletPadV2 as Proxy>::Event,
        data: &PadData,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use sctk::reexports::protocols::wp::tablet::zv2::client::zwp_tablet_pad_v2::Event;
        match event {
            Event::Group { pad_group } => data.0.lock().unwrap().push(pad_group),
            Event::Removed => {
                for group in data.0.lock().unwrap().drain(..) {
                    if let Some(children) = group.data::<GroupData>() {
                        for ring in children.rings.lock().unwrap().drain(..) {
                            ring.destroy();
                        }
                        for strip in children.strips.lock().unwrap().drain(..) {
                            strip.destroy();
                        }
                    }
                    group.destroy();
                }
                pad.destroy();
            }
            _ => {}
        }
    }
    event_created_child!(WinitState, ZwpTabletPadV2, [0 => (ZwpTabletPadGroupV2, GroupData::default())]);
}

impl Dispatch<ZwpTabletPadGroupV2, GroupData> for WinitState {
    fn event(
        _: &mut Self,
        _: &ZwpTabletPadGroupV2,
        event: <ZwpTabletPadGroupV2 as Proxy>::Event,
        data: &GroupData,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use sctk::reexports::protocols::wp::tablet::zv2::client::zwp_tablet_pad_group_v2::Event;
        match event {
            Event::Ring { ring } => data.rings.lock().unwrap().push(ring),
            Event::Strip { strip } => data.strips.lock().unwrap().push(strip),
            _ => {}
        }
    }
    event_created_child!(WinitState, ZwpTabletPadGroupV2, [1 => (ZwpTabletPadRingV2, ()), 2 => (ZwpTabletPadStripV2, ())]);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn leaving_proximity_releases_side_buttons_without_a_late_duplicate() {
        let mut input = ToolInput::default();
        input.button(MouseButton::Middle, ElementState::Pressed);
        input.release_buttons();
        input.button(MouseButton::Middle, ElementState::Released);
        assert!(input.held_buttons.is_empty());
        assert_eq!(
            input.buttons,
            vec![
                (ElementState::Pressed, MouseButton::Middle),
                (ElementState::Released, MouseButton::Middle)
            ]
        );
    }
}
