// Generado desde los planos oficiales (Arduino .brd EAGLE, PDF mecánicos de
// Raspberry Pi, Espressif, NodeMCU, Wemos): ver library/boards.rs.
// Origen en la esquina inferior izquierda, componentes hacia arriba.

use super::boards::{Board, Comp, Edge};

pub(crate) const BOARDS: &[Board] = &[
    Board {
        id: "arduino_uno",
        name: "Arduino Uno R3",
        size: (68.58, 53.34),
        thickness: 1.6,
        corner: 1.0,
        outline: &[(1.0, 0.0), (65.04, 0.0), (66.04, 1.0), (66.04, 2.54), (68.58, 5.08), (68.58, 37.846), (66.04, 40.386), (66.04, 51.816), (64.516, 53.34), (1.0, 53.34), (0.0, 52.34), (0.0, 1.0)],
        holes: &[(13.97, 2.54, 3.2), (15.24, 50.8, 3.2), (66.04, 7.62, 3.2), (66.04, 35.56, 3.2)],
        comps: &[
            // USB-B
            Comp { x: (-6.34, 9.41), y: (32.2, 44.0), h: 11.0, edge: Edge::Left, below: false },
            // DC power jack 2.1x5.5
            Comp { x: (-1.8, 11.68), y: (3.3, 12.29), h: 11.0, edge: Edge::Left, below: false },
            // Reset button
            Comp { x: (2.15, 10.55), y: (47.15, 51.65), h: 3.5, edge: Edge::None, below: false },
            // Digital low D0-D7 (IOL)
            Comp { x: (44.45, 64.77), y: (49.53, 52.07), h: 8.5, edge: Edge::None, below: false },
            // Digital high D8-D13+GND+AREF+SDA+SCL (IOH)
            Comp { x: (17.53, 42.93), y: (49.53, 52.07), h: 8.5, edge: Edge::None, below: false },
            // Power (IOREF..VIN)
            Comp { x: (26.67, 46.99), y: (1.27, 3.81), h: 8.5, edge: Edge::None, below: false },
            // Analog A0-A5
            Comp { x: (49.53, 64.77), y: (1.27, 3.81), h: 8.5, edge: Edge::None, below: false },
            // ICSP (ATmega328)
            Comp { x: (62.36, 67.44), y: (24.13, 31.75), h: 8.5, edge: Edge::None, below: false },
            // ICSP1 (16U2)
            Comp { x: (14.48, 22.1), y: (43.69, 48.77), h: 8.5, edge: Edge::None, below: false },
        ],
    },
    Board {
        id: "arduino_mega",
        name: "Arduino Mega 2560",
        size: (101.6, 53.34),
        thickness: 1.6,
        corner: 1.0,
        outline: &[(1.0, 0.0), (98.06, 0.0), (99.06, 1.0), (99.06, 2.54), (101.6, 5.08), (101.6, 37.846), (99.06, 40.386), (99.06, 51.816), (97.536, 53.34), (1.0, 53.34), (0.0, 52.34), (0.0, 1.0)],
        holes: &[(13.97, 2.54, 3.2), (15.24, 50.8, 3.2), (66.04, 7.62, 3.2), (66.04, 35.56, 3.2), (90.17, 50.8, 3.2), (96.52, 2.54, 3.2)],
        comps: &[
            // USB-B
            Comp { x: (-6.34, 9.41), y: (32.2, 44.0), h: 11.0, edge: Edge::Left, below: false },
            // DC power jack 2.1x5.5
            Comp { x: (-1.8, 11.68), y: (3.3, 12.29), h: 11.0, edge: Edge::Left, below: false },
            // Reset button
            Comp { x: (2.02, 10.42), y: (45.77, 51.77), h: 3.5, edge: Edge::None, below: false },
            // D22-D53 2x18 block (XIO)
            Comp { x: (92.71, 97.79), y: (6.35, 52.07), h: 8.5, edge: Edge::None, below: false },
            // Digital D0..(JP6 1x10)
            Comp { x: (17.53, 42.93), y: (49.53, 52.07), h: 8.5, edge: Edge::None, below: false },
            // PWM low (PWML)
            Comp { x: (44.45, 64.77), y: (49.53, 52.07), h: 8.5, edge: Edge::None, below: false },
            // Communication
            Comp { x: (67.31, 87.63), y: (49.53, 52.07), h: 8.5, edge: Edge::None, below: false },
            // Power
            Comp { x: (26.67, 46.99), y: (1.27, 3.81), h: 8.5, edge: Edge::None, below: false },
            // Analog low A0-A7 (ADCL)
            Comp { x: (49.53, 69.85), y: (1.27, 3.81), h: 8.5, edge: Edge::None, below: false },
            // Analog high A8-A15 (ADCH)
            Comp { x: (72.39, 92.71), y: (1.27, 3.81), h: 8.5, edge: Edge::None, below: false },
        ],
    },
    Board {
        id: "arduino_nano",
        name: "Arduino Nano",
        size: (43.2, 18.0),
        thickness: 1.6,
        corner: 0.0,
        outline: &[],
        holes: &[(1.28, 1.38, 1.8), (41.92, 1.38, 1.8), (1.28, 16.62, 1.8), (41.92, 16.62, 1.8)],
        comps: &[
            // Mini-USB B
            Comp { x: (-0.5, 6.9), y: (6.9, 14.7), h: 4.0, edge: Edge::Left, below: false },
            // Row A (D12..D1/TX, GND, RST, ...)
            Comp { x: (2.55, 40.65), y: (0.1, 2.66), h: 8.5, edge: Edge::None, below: true },
            // Row B (D13.. +3V3, REF, A0..)
            Comp { x: (2.55, 40.65), y: (15.34, 17.9), h: 8.5, edge: Edge::None, below: true },
            // ICSP 2x3
            Comp { x: (37.7, 43.2), y: (5.1, 14.7), h: 8.5, edge: Edge::None, below: true },
        ],
    },
    Board {
        id: "arduino_pro_mini",
        name: "Arduino Pro Mini",
        size: (17.78, 33.02),
        thickness: 0.8,
        corner: 0.0,
        outline: &[(0.0, 0.0), (17.78, 0.0), (17.78, 33.02), (0.0, 33.02)],
        holes: &[],
        comps: &[
            // Left pin row JP7 (12 pins)
            Comp { x: (0.0, 2.54), y: (0.0, 30.48), h: 8.5, edge: Edge::None, below: true },
            // Right pin row JP6 (12 pins)
            Comp { x: (15.24, 17.78), y: (0.0, 30.48), h: 8.5, edge: Edge::None, below: true },
            // FTDI header JP1 (6 pins)
            Comp { x: (1.27, 16.51), y: (30.48, 33.02), h: 8.5, edge: Edge::None, below: true },
            // Reset button
            Comp { x: (6.22, 11.81), y: (0.76, 4.57), h: 3.5, edge: Edge::None, below: false },
        ],
    },
    Board {
        id: "rpi5",
        name: "Raspberry Pi 5",
        size: (85.0, 56.0),
        thickness: 1.6,
        corner: 3.0,
        outline: &[],
        holes: &[(3.5, 3.5, 2.7), (61.5, 3.5, 2.7), (3.5, 52.5, 2.7), (61.5, 52.5, 2.7)],
        comps: &[
            // USB-C power
            Comp { x: (6.8, 15.6), y: (-1.1, 6.0), h: 3.2, edge: Edge::Bottom, below: false },
            // micro-HDMI 0
            Comp { x: (22.6, 29.0), y: (-1.7, 6.5), h: 3.0, edge: Edge::Bottom, below: false },
            // micro-HDMI 1
            Comp { x: (36.0, 42.4), y: (-1.7, 6.5), h: 3.0, edge: Edge::Bottom, below: false },
            // Ethernet RJ45
            Comp { x: (66.5, 88.2), y: (2.1, 18.3), h: 13.5, edge: Edge::Right, below: false },
            // USB 3.0 stack
            Comp { x: (70.4, 87.8), y: (22.4, 35.5), h: 16.0, edge: Edge::Right, below: false },
            // USB 2.0 stack
            Comp { x: (70.4, 87.8), y: (40.5, 53.5), h: 16.0, edge: Edge::Right, below: false },
            // GPIO 40-pin
            Comp { x: (7.1, 57.9), y: (49.96, 55.04), h: 8.5, edge: Edge::None, below: false },
        ],
    },
    Board {
        id: "rpi4",
        name: "Raspberry Pi 4 Model B",
        size: (85.0, 56.0),
        thickness: 1.6,
        corner: 3.0,
        outline: &[],
        holes: &[(3.5, 3.5, 2.7), (61.5, 3.5, 2.7), (3.5, 52.5, 2.7), (61.5, 52.5, 2.7)],
        comps: &[
            // USB-C power
            Comp { x: (6.8, 15.6), y: (-1.1, 6.0), h: 3.2, edge: Edge::Bottom, below: false },
            // micro-HDMI 0
            Comp { x: (23.1, 29.3), y: (-1.7, 6.5), h: 3.0, edge: Edge::Bottom, below: false },
            // micro-HDMI 1
            Comp { x: (36.3, 42.7), y: (-1.7, 6.5), h: 3.0, edge: Edge::Bottom, below: false },
            // 3.5 mm audio/composite jack
            Comp { x: (50.0, 57.0), y: (-2.8, 12.4), h: 6.0, edge: Edge::Bottom, below: false },
            // Camera (CSI) FPC
            Comp { x: (45.0, 48.6), y: (0.2, 22.3), h: 5.5, edge: Edge::None, below: false },
            // Display (DSI) FPC
            Comp { x: (1.4, 5.3), y: (16.3, 39.4), h: 5.5, edge: Edge::None, below: false },
            // Ethernet RJ45
            Comp { x: (66.7, 88.1), y: (38.0, 53.5), h: 13.5, edge: Edge::Right, below: false },
            // USB 3.0 stack (2 ports)
            Comp { x: (70.6, 87.5), y: (20.2, 33.6), h: 16.0, edge: Edge::Right, below: false },
            // USB 2.0 stack (2 ports)
            Comp { x: (70.6, 87.5), y: (2.25, 15.5), h: 16.0, edge: Edge::Right, below: false },
            // GPIO 40-pin
            Comp { x: (7.1, 57.9), y: (49.96, 55.04), h: 8.5, edge: Edge::None, below: false },
        ],
    },
    Board {
        id: "rpi3bp",
        name: "Raspberry Pi 3 Model B+",
        size: (85.0, 56.0),
        thickness: 1.6,
        corner: 3.0,
        outline: &[],
        holes: &[(3.5, 3.5, 2.75), (61.5, 3.5, 2.75), (3.5, 52.5, 2.75), (61.5, 52.5, 2.75)],
        comps: &[
            // micro-USB power
            Comp { x: (6.8, 14.4), y: (-1.5, 4.8), h: 3.0, edge: Edge::Bottom, below: false },
            // HDMI
            Comp { x: (25.0, 39.0), y: (-1.5, 10.6), h: 6.5, edge: Edge::Bottom, below: false },
            // 3.5 mm jack
            Comp { x: (50.0, 57.0), y: (-3.5, 13.2), h: 6.0, edge: Edge::Bottom, below: false },
            // Camera (CSI)
            Comp { x: (43.6, 47.0), y: (-0.4, 22.5), h: 5.5, edge: Edge::None, below: false },
            // Display (DSI)
            Comp { x: (2.1, 6.1), y: (16.9, 39.2), h: 5.5, edge: Edge::None, below: false },
            // Ethernet RJ45
            Comp { x: (65.8, 87.6), y: (2.1, 17.9), h: 13.5, edge: Edge::Right, below: false },
            // USB 2.0 stack A
            Comp { x: (69.7, 87.1), y: (22.5, 35.7), h: 16.0, edge: Edge::Right, below: false },
            // USB 2.0 stack B
            Comp { x: (69.7, 87.1), y: (40.3, 53.9), h: 16.0, edge: Edge::Right, below: false },
            // GPIO 40-pin
            Comp { x: (7.1, 57.9), y: (49.96, 55.04), h: 8.5, edge: Edge::None, below: false },
        ],
    },
    Board {
        id: "rpi_zero2w",
        name: "Raspberry Pi Zero 2 W",
        size: (65.0, 30.0),
        thickness: 1.6,
        corner: 0.0,
        outline: &[],
        holes: &[(3.5, 3.5, 2.75), (61.5, 3.5, 2.75), (3.5, 26.5, 2.75), (61.5, 26.5, 2.75)],
        comps: &[
            // mini-HDMI
            Comp { x: (6.8, 18.0), y: (-0.7, 6.9), h: 3.5, edge: Edge::Bottom, below: false },
            // micro-USB (data)
            Comp { x: (37.4, 45.4), y: (-1.2, 5.3), h: 3.0, edge: Edge::Bottom, below: false },
            // micro-USB (power)
            Comp { x: (50.0, 58.0), y: (-1.2, 5.3), h: 3.0, edge: Edge::Bottom, below: false },
        ],
    },
    Board {
        id: "rpi_pico",
        name: "Raspberry Pi Pico",
        size: (51.0, 21.0),
        thickness: 1.0,
        corner: 0.0,
        outline: &[],
        holes: &[(2.0, 4.8, 2.1), (2.0, 16.2, 2.1), (49.0, 4.8, 2.1), (49.0, 16.2, 2.1)],
        comps: &[
            // micro-USB B
            Comp { x: (-1.3, 6.0), y: (6.5, 14.5), h: 3.0, edge: Edge::Left, below: false },
            // Left/bottom pin row (pins 1-20)
            Comp { x: (0.1, 50.9), y: (0.34, 2.88), h: 8.5, edge: Edge::None, below: true },
            // Right/top pin row (pins 21-40)
            Comp { x: (0.1, 50.9), y: (18.12, 20.66), h: 8.5, edge: Edge::None, below: true },
        ],
    },
    Board {
        id: "esp32_devkitc",
        name: "ESP32-DevKitC V4",
        size: (27.94, 48.26),
        thickness: 1.6,
        corner: 0.0,
        outline: &[],
        holes: &[],
        comps: &[
            // Micro-USB B
            Comp { x: (9.8, 18.1), y: (-1.2, 6.1), h: 3.0, edge: Edge::Bottom, below: false },
            // J2 left header (19 pins)
            Comp { x: (0.0, 2.54), y: (0.02, 48.28), h: 8.5, edge: Edge::None, below: true },
            // J3 right header (19 pins)
            Comp { x: (25.4, 27.94), y: (0.02, 48.28), h: 8.5, edge: Edge::None, below: true },
            // ESP32-WROOM-32 module
            Comp { x: (4.97, 22.97), y: (22.8, 54.3), h: 3.2, edge: Edge::None, below: false },
        ],
    },
    Board {
        id: "nodemcu_v2",
        name: "NodeMCU ESP8266 v2 (Amica)",
        size: (25.4, 48.26),
        thickness: 1.6,
        corner: 0.0,
        outline: &[(0.0, 2.286), (2.286, 0.0), (23.114, 0.0), (25.4, 2.286), (25.4, 45.974), (23.114, 48.26), (2.286, 48.26), (0.0, 45.974)],
        holes: &[(2.29, 2.29, 2.92), (23.11, 2.29, 2.92), (2.29, 45.97, 2.92), (23.11, 45.97, 2.92)],
        comps: &[
            // Micro-USB B
            Comp { x: (8.95, 16.45), y: (-1.2, 5.5), h: 3.0, edge: Edge::Bottom, below: false },
            // Left header (15 pins)
            Comp { x: (0.0, 2.54), y: (5.08, 43.18), h: 8.5, edge: Edge::None, below: true },
            // Right header (15 pins)
            Comp { x: (22.86, 25.4), y: (5.08, 43.18), h: 8.5, edge: Edge::None, below: true },
            // ESP-12E module
            Comp { x: (4.7, 20.7), y: (24.2, 48.26), h: 3.2, edge: Edge::None, below: false },
        ],
    },
    Board {
        id: "d1_mini",
        name: "Wemos D1 mini",
        size: (25.4, 34.3),
        thickness: 1.0,
        corner: 3.8,
        outline: &[],
        holes: &[(2.5, 31.04, 2.0), (22.9, 31.04, 2.0)],
        comps: &[
            // USB-C
            Comp { x: (7.9, 17.5), y: (-1.2, 5.5), h: 3.2, edge: Edge::Bottom, below: false },
            // Left pin row (8 pins)
            Comp { x: (0.0, 2.54), y: (7.9, 28.2), h: 8.5, edge: Edge::None, below: true },
            // Right pin row (8 pins)
            Comp { x: (22.86, 25.4), y: (7.9, 28.2), h: 8.5, edge: Edge::None, below: true },
        ],
    },
];
