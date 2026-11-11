//! Every DualSense control that can trigger a pad.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Input {
    Cross,
    Circle,
    Square,
    Triangle,
    Up,
    Down,
    Left,
    Right,
    L1,
    R1,
    L2,
    R2,
    L3,
    R3,
    Create,
    Options,
    Ps,
    Mute,
    TouchLeft,
    TouchRight,
}

impl Input {
    pub const ALL: [Input; 20] = [
        Input::Cross,
        Input::Circle,
        Input::Square,
        Input::Triangle,
        Input::Up,
        Input::Down,
        Input::Left,
        Input::Right,
        Input::L1,
        Input::R1,
        Input::L2,
        Input::R2,
        Input::L3,
        Input::R3,
        Input::Create,
        Input::Options,
        Input::Ps,
        Input::Mute,
        Input::TouchLeft,
        Input::TouchRight,
    ];

    /// Every control can play a pad except PS, which Windows and Steam keep for themselves.
    pub fn bindable(self) -> bool {
        self != Input::Ps
    }

    pub fn bit(self) -> u32 {
        1 << (self as u32)
    }

    /// Short text for compact badges (face buttons and the d-pad are drawn as shapes instead).
    pub fn short(self) -> &'static str {
        match self {
            Input::L1 => "L1",
            Input::R1 => "R1",
            Input::L2 => "L2",
            Input::R2 => "R2",
            Input::L3 => "L3",
            Input::R3 => "R3",
            Input::Create => "CR",
            Input::Options => "OP",
            Input::Ps => "PS",
            Input::Mute => "MU",
            Input::TouchLeft => "TL",
            Input::TouchRight => "TR",
            _ => "",
        }
    }
}
