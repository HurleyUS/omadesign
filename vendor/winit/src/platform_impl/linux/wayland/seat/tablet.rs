//! Native tablet tools share the window pointer route and retain pen pressure.

use crate::dpi::LogicalPosition;
use crate::event::{ElementState, Force, MouseButton, Touch, TouchPhase, WindowEvent};
use crate::platform_impl::wayland::state::WinitState;
use crate::platform_impl::wayland::{self, DeviceId};
use sctk::globals::GlobalData;
use sctk::reexports::client::globals::{BindError, GlobalList};
use sctk::reexports::client::protocol::wl_seat::WlSeat;
use sctk::reexports::client::{Connection, Dispatch, Proxy, QueueHandle, event_created_child};
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
struct ToolInput {
    window: Option<wayland::WindowId>,
    location: LogicalPosition<f64>,
    pressure: Option<f64>,
    down: bool,
    phase: Option<TouchPhase>,
    leaving: bool,
    buttons: Vec<(ElementState, MouseButton)>,
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
    event_created_child!(WinitState, ZwpTabletSeatV2, [0 => (ZwpTabletV2, ()), 1 => (ZwpTabletToolV2, ToolData::default()), 2 => (ZwpTabletPadV2, ())]);
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
                input.buttons.push((state, button));
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
                if let Some(phase) = input
                    .phase
                    .take()
                    .or(input.down.then_some(TouchPhase::Moved))
                {
                    state.events_sink.push_window_event(
                        WindowEvent::Touch(Touch {
                            device_id,
                            phase,
                            location: input.location.to_physical(scale),
                            force: input.pressure.map(Force::Normalized),
                            id: tool.id().protocol_id() as u64,
                        }),
                        window_id,
                    );
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
ignore_tablet_object!(ZwpTabletV2);
ignore_tablet_object!(ZwpTabletPadRingV2);
ignore_tablet_object!(ZwpTabletPadStripV2);

impl Dispatch<ZwpTabletPadV2, ()> for WinitState {
    fn event(
        _: &mut Self,
        _: &ZwpTabletPadV2,
        _: <ZwpTabletPadV2 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
    event_created_child!(WinitState, ZwpTabletPadV2, [0 => (ZwpTabletPadGroupV2, ())]);
}
impl Dispatch<ZwpTabletPadGroupV2, ()> for WinitState {
    fn event(
        _: &mut Self,
        _: &ZwpTabletPadGroupV2,
        _: <ZwpTabletPadGroupV2 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
    event_created_child!(WinitState, ZwpTabletPadGroupV2, [1 => (ZwpTabletPadRingV2, ()), 2 => (ZwpTabletPadStripV2, ())]);
}
