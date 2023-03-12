use crate::save_load::*;

pub struct VolumeEnvelopeState {
    // Volume Envelope
    pub volume_register: u8,
    pub decay: u8,
    pub divider: u8,
    pub enabled: bool,
    pub looping: bool,
    pub start_flag: bool,
}

impl VolumeEnvelopeState {
    pub fn new() -> VolumeEnvelopeState {
        return VolumeEnvelopeState {
            volume_register: 0,
            decay: 0,
            divider: 0,
            enabled: false,
            looping: false,
            start_flag: false,
        }
    }

    pub fn current_volume(&self) -> u8 {
        if self.enabled {
            return self.decay;
        } else {
            return self.volume_register;
        }
    }

    pub fn clock(&mut self) {
        if self.start_flag {
            self.decay = 15;
            self.start_flag = false;
            self.divider = self.volume_register;
        } else {
            // Clock the divider
            if self.divider == 0 {
                self.divider = self.volume_register;
                if self.decay > 0 {
                    self.decay -= 1;
                } else {
                    if self.looping {
                        self.decay = 15;
                    }
                }
            } else {
                self.divider = self.divider - 1;
            }
        }
    }

    pub fn save_state(&self, buff: &mut Vec<u8>) {
        save_u8(buff, self.volume_register);
        save_u8(buff, self.decay);
        save_u8(buff, self.divider);
        save_bool(buff, self.enabled);
        save_bool(buff, self.looping);
        save_bool(buff, self.start_flag);
    }

    pub fn load_state(&mut self, buff: &mut Vec<u8>) {
        load_bool(buff, &mut self.start_flag);
        load_bool(buff, &mut self.looping);
        load_bool(buff, &mut self.enabled);
        load_u8(buff, &mut self.divider);
        load_u8(buff, &mut self.decay);
        load_u8(buff, &mut self.volume_register);
    }
}