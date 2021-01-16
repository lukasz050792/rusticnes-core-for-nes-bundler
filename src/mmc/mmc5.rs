// Most powerful Nintendo produced mapper, supporting many advanced features
// As RusticNES doesn't support expansion audio, I'm not bothering to implement
// it here quite yet.
// Reference capabilities: https://wiki.nesdev.com/w/index.php/MMC5

use cartridge::NesHeader;
use mmc::mapper::*;
use std::cmp::min;
use std::cmp::max;
use apu::PulseChannelState;

#[derive(Copy, Clone, PartialEq)]
pub enum PpuMode {
    Backgrounds,
    Sprites,
    PpuData
}

pub struct Mmc5 {
    pub prg_rom: Vec<u8>,
    pub prg_ram: Vec<u8>,
    pub chr_rom: Vec<u8>,
    pub mirroring: Mirroring,
    pub ppuctrl_monitor: u8,
    pub ppumask_monitor: u8,
    pub prg_mode: u8,
    pub chr_mode: u8,
    pub prg_ram_magic_low: u8,
    pub prg_ram_magic_high: u8,
    pub extended_ram_mode: u8,
    pub vram: Vec<u8>,
    pub extram: Vec<u8>,
    pub nametable_mapping: u8,
    pub fill_tile: u8,
    pub fill_attr: u8,
    pub prg_bank_a_isram: bool,
    pub prg_bank_b_isram: bool,
    pub prg_bank_c_isram: bool,
    pub prg_bank_a: u8,
    pub prg_bank_b: u8,
    pub prg_bank_c: u8,
    pub prg_bank_d: u8,
    pub prg_ram_bank: u8,
    pub chr_banks: Vec<usize>,
    pub chr_ext_banks: Vec<usize>,
    pub chr_last_write_ext: bool,
    pub ppu_read_mode: PpuMode,
    pub chr_bank_high_bits: usize,
    pub irq_scanline_compare: u8,
    pub irq_enabled: bool,
    pub irq_pending: bool,
    pub in_frame: bool,
    pub current_scanline: u8,
    pub last_ppu_fetch: u16,
    pub last_bg_tile_fetch: u16,
    pub consecutive_nametable_count: u8,
    pub cpu_cycles_since_last_ppu_read: u8,
    pub ppu_fetches_this_scanline: u16,
    pub multiplicand_a: u8,
    pub multiplicand_b: u8,
    pub pulse_1: PulseChannelState,
    pub pulse_2: PulseChannelState,
    pub pcm_level: u8,
    pub pcm_read_mode: bool,
    pub pcm_irq_enable: bool,
    pub pcm_irq_pending: bool,
    pub audio_sequencer_counter: u16,
}

fn banked_memory_index(data_store_length: usize, bank_size: usize, bank_number: usize, raw_address: usize) -> usize {
    let total_banks = max(data_store_length / bank_size, 1);
    let selected_bank = bank_number % total_banks;
    let bank_start_offset = bank_size * selected_bank;
    let offset_within_bank = raw_address % min(bank_size, data_store_length);
    return bank_start_offset + offset_within_bank;
}

impl Mmc5 {
    pub fn new(header: NesHeader, chr: &[u8], prg: &[u8]) -> Mmc5 {
        let chr_rom = match header.has_chr_ram {
            true => vec![0u8; 8 * 1024],
            false => chr.to_vec()
        };

        return Mmc5 {
            prg_rom: prg.to_vec(),
            prg_ram: vec![0u8; 64 * 1024],
            chr_rom: chr_rom,
            mirroring: header.mirroring,
            ppuctrl_monitor: 0,
            ppumask_monitor: 0,
            prg_mode: 3,   // Koei games require MMC5 to boot into PRG mode 3
            chr_mode: 0,
            prg_ram_magic_low: 0,
            prg_ram_magic_high: 0,
            extended_ram_mode: 0,
            vram: vec![0u8; 0x1000],
            extram: vec![0u8; 0x800],
            nametable_mapping: 0,
            fill_tile: 0,
            fill_attr: 0,
            prg_bank_a: 0,
            prg_bank_b: 0,
            prg_bank_c: 0,
            prg_bank_d: 0x7F,   // Defaults to 0xFF, so interrupt vectors are loaded at boot
            prg_ram_bank: 0,
            prg_bank_a_isram: false,
            prg_bank_b_isram: false,
            prg_bank_c_isram: false,
            chr_banks: vec![0usize; 8],
            chr_ext_banks: vec![0usize; 8],
            chr_last_write_ext: false,
            ppu_read_mode: PpuMode::PpuData,
            chr_bank_high_bits: 0,
            irq_scanline_compare: 0,
            irq_enabled: false,
            irq_pending: false,
            in_frame: false,
            current_scanline: 0,
            last_ppu_fetch: 0,
            last_bg_tile_fetch: 0,
            consecutive_nametable_count: 0,
            cpu_cycles_since_last_ppu_read: 0,
            ppu_fetches_this_scanline: 0,
            multiplicand_a: 0xFF,
            multiplicand_b: 0xFF,
            pulse_1: PulseChannelState::new(false),
            pulse_2: PulseChannelState::new(false),
            pcm_level: 0,
            pcm_read_mode: false,
            pcm_irq_enable: false,
            pcm_irq_pending: false,
            audio_sequencer_counter: 0,
        }
    }

    pub fn large_sprites_active(&self) -> bool {
        return ((self.ppuctrl_monitor & 0b0010_0000) != 0) && ((self.ppumask_monitor & 0b0001_1000) != 0);
    }

    pub fn prg_ram_write_enabled(&self) -> bool {
        return (self.prg_ram_magic_low == 0b10) && (self.prg_ram_magic_high == 0b01);
    }

    // Nametable mapping helper functions, to assist with MMC5's arbitrary quadrant mapping
    pub fn nametable_vram_low(&self, address: u16) -> u8 {
        let masked_address = address & 0x3FF;
        return self.vram[masked_address as usize];
    }

    pub fn nametable_vram_high(&self, address: u16) -> u8 {
        let masked_address = address & 0x3FF;
        return self.vram[masked_address as usize + 0x400];
    }

    pub fn nametable_ext1(&self, address: u16) -> u8 {
        if self.extended_ram_mode == 0 || self.extended_ram_mode == 1 {
            let masked_address = address & 0x3FF;
            return self.extram[masked_address as usize];
        } else {
            return 0;
        }
    }

    pub fn nametable_fixed(&self, address: u16) -> u8 {
        let masked_address = address & 0x3FF;
        if masked_address < 0x3C0 {
            return self.fill_tile;
        } else {
            return self.fill_attr;
        }
    }

    pub fn read_nametable(&self, address: u16) -> u8 {
        let masked_address = address & 0xFFF;
        let quadrant = masked_address / 0x400;
        let nametable_select = (self.nametable_mapping >> (quadrant * 2)) & 0b11;
        return match nametable_select {
            0 => self.nametable_vram_low(masked_address),
            1 => self.nametable_vram_high(masked_address),
            2 => self.nametable_ext1(masked_address),
            3 => self.nametable_fixed(masked_address),
            _ => 0 // Shouldn't be reachable
        }
    }

    pub fn write_nametable(&mut self, address: u16, data: u8) {
        let address_within_nametables = address & 0xFFF;
        let address_within_quadrant = address & 0x3FF;
        let quadrant = address_within_nametables / 0x400;
        let nametable_select = (self.nametable_mapping >> (quadrant * 2)) & 0b11;
        match nametable_select {
            0 => {self.vram[address_within_quadrant as usize] = data;},
            1 => {self.vram[address_within_quadrant as usize + 0x400] = data;},
            2 => {
                if self.extended_ram_mode == 0 || self.extended_ram_mode == 1 {
                    self.extram[address_within_quadrant as usize] = data;
                }
            },
            _ => {}
        }
    }

    pub fn read_prg_mode_0(&self, address: u16) -> u8 {
        let (datastore, bank_number, bank_size) = match address {
            0x6000 ..= 0x7FFF => (&self.prg_ram, self.prg_ram_bank, 8 * 1024),
            0x8000 ..= 0xFFFF => (&self.prg_rom, self.prg_bank_d >> 2, 32 * 1024),
            _ => {return 0}
        };

        let datastore_offset = banked_memory_index(datastore.len(), bank_size, bank_number as usize, address as usize);
        return datastore[datastore_offset];
    }

    pub fn read_prg_mode_1(&self, address: u16) -> u8 {
        let (datastore, bank_number, bank_size) = match address {
            0x6000 ..= 0x7FFF => (&self.prg_ram, self.prg_ram_bank, 8 * 1024),
            0x8000 ..= 0xBFFF => match self.prg_bank_b_isram {
                true  => (&self.prg_ram, self.prg_bank_b >> 1, 16 * 1024),
                false => (&self.prg_rom, self.prg_bank_b >> 1, 16 * 1024)
            },
            0xC000 ..= 0xFFFF => (&self.prg_rom, self.prg_bank_d >> 1, 16 * 1024),
            _ => {return 0}
        };

        let datastore_offset = banked_memory_index(datastore.len(), bank_size, bank_number as usize, address as usize);
        return datastore[datastore_offset];
    }

    pub fn read_prg_mode_2(&self, address: u16) -> u8 {
        let (datastore, bank_number, bank_size) = match address {
            0x6000 ..= 0x7FFF => (&self.prg_ram, self.prg_ram_bank, 8 * 1024),
            0x8000 ..= 0xBFFF => match self.prg_bank_b_isram {
                true  => (&self.prg_ram, self.prg_bank_b >> 1, 16 * 1024),
                false => (&self.prg_rom, self.prg_bank_b >> 1, 16 * 1024)
            },
            0xC000 ..= 0xDFFF => match self.prg_bank_c_isram {
                true  => (&self.prg_ram, self.prg_bank_c, 8 * 1024),
                false => (&self.prg_rom, self.prg_bank_c, 8 * 1024)
            },
            0xE000 ..= 0xFFFF => (&self.prg_rom, self.prg_bank_d, 8 * 1024),
            _ => {return 0}
        };

        let datastore_offset = banked_memory_index(datastore.len(), bank_size, bank_number as usize, address as usize);
        return datastore[datastore_offset];
    }

    pub fn read_prg_mode_3(&self, address: u16) -> u8 {
        let (datastore, bank_number, bank_size) = match address {
            0x6000 ..= 0x7FFF => (&self.prg_ram, self.prg_ram_bank, 8 * 1024),
            0x8000 ..= 0x9FFF => match self.prg_bank_a_isram {
                true  => (&self.prg_ram, self.prg_bank_a, 8 * 1024),
                false => (&self.prg_rom, self.prg_bank_a, 8 * 1024)
            },
            0xA000 ..= 0xBFFF => match self.prg_bank_b_isram {
                true  => (&self.prg_ram, self.prg_bank_b, 8 * 1024),
                false => (&self.prg_rom, self.prg_bank_b, 8 * 1024)
            },
            0xC000 ..= 0xDFFF => match self.prg_bank_c_isram {
                true  => (&self.prg_ram, self.prg_bank_c, 8 * 1024),
                false => (&self.prg_rom, self.prg_bank_c, 8 * 1024)
            },
            0xE000 ..= 0xFFFF => (&self.prg_rom, self.prg_bank_d, 8 * 1024),
            _ => {return 0}
        };

        let datastore_offset = banked_memory_index(datastore.len(), bank_size, bank_number as usize, address as usize);
        return datastore[datastore_offset];
    }

    pub fn read_prg(&self, address: u16) -> u8 {
        return match self.prg_mode {
            0 => self.read_prg_mode_0(address),
            1 => self.read_prg_mode_1(address),
            2 => self.read_prg_mode_2(address),
            3 => self.read_prg_mode_3(address),
            _ => 0 // Should be unreachable
        }
    }

    pub fn write_prg_mode_0(&mut self, address: u16, data: u8) {
        let (bank_number, bank_size) = match address {
            0x6000 ..= 0x7FFF => (self.prg_ram_bank, 8 * 1024),
            _ => {return}
        };

        let datastore_offset = banked_memory_index(self.prg_ram.len(), bank_size, bank_number as usize, address as usize);
        self.prg_ram[datastore_offset] = data;
    }

    pub fn write_prg_mode_1(&mut self, address: u16, data: u8) {
        let (bank_number, bank_size) = match address {
            0x6000 ..= 0x7FFF => (self.prg_ram_bank, 8 * 1024),
            0x8000 ..= 0xBFFF => match self.prg_bank_b_isram {
                true  => (self.prg_bank_b >> 1, 16 * 1024),
                false => {return}
            },
            _ => {return}
        };

        let datastore_offset = banked_memory_index(self.prg_ram.len(), bank_size, bank_number as usize, address as usize);
        self.prg_ram[datastore_offset] = data;
    }

    pub fn write_prg_mode_2(&mut self, address: u16, data: u8) {
        let (bank_number, bank_size) = match address {
            0x6000 ..= 0x7FFF => (self.prg_ram_bank, 8 * 1024),
            0x8000 ..= 0xBFFF => match self.prg_bank_b_isram {
                true  => (self.prg_bank_b >> 1, 16 * 1024),
                false => {return}
            },
            0xC000 ..= 0xDFFF => match self.prg_bank_c_isram {
                true  => (self.prg_bank_c, 8 * 1024),
                false => {return}
            },
            _ => {return}
        };

        let datastore_offset = banked_memory_index(self.prg_ram.len(), bank_size, bank_number as usize, address as usize);
        self.prg_ram[datastore_offset] = data;
    }

    pub fn write_prg_mode_3(&mut self, address: u16, data: u8) {
        let (bank_number, bank_size) = match address {
            0x6000 ..= 0x7FFF => (self.prg_ram_bank, 8 * 1024),
            0x8000 ..= 0x9FFF => match self.prg_bank_a_isram {
                true  => (self.prg_bank_a, 8 * 1024),
                false => {return}
            },
            0xA000 ..= 0xBFFF => match self.prg_bank_b_isram {
                true  => (self.prg_bank_b, 8 * 1024),
                false => {return}
            },
            0xC000 ..= 0xDFFF => match self.prg_bank_c_isram {
                true  => (self.prg_bank_c, 8 * 1024),
                false => {return}
            },
            _ => {return}
        };

        let datastore_offset = banked_memory_index(self.prg_ram.len(), bank_size, bank_number as usize, address as usize);
        self.prg_ram[datastore_offset] = data;
    }

    pub fn write_prg(&mut self, address: u16, data: u8) {
        match self.prg_mode {
            0 => self.write_prg_mode_0(address, data),
            1 => self.write_prg_mode_1(address, data),
            2 => self.write_prg_mode_2(address, data),
            3 => self.write_prg_mode_3(address, data),
            _ => {} // Should be unreachable
        }
    }

    pub fn read_banked_chr(&self, address: u16) -> u8 {
        let chr_bank_size = match self.chr_mode {
            0 => 8192,
            1 => 4096,
            2 => 2048,
            3 => 1024,
            _ => return 0
        };

        let chr_region = address / chr_bank_size;
        let standard_bank_index = (chr_region + 1) * (8 >> self.chr_mode) - 1;
        let extended_bank_index = standard_bank_index & 0x3;

        let large_sprites_enabled = (self.ppuctrl_monitor & 0b0010_0000) != 0;
        let currently_reading_backgrounds = self.ppu_read_mode == PpuMode::Backgrounds;
        let ppu_inactive = self.ppu_read_mode == PpuMode::PpuData;
        let wrote_ext_register_last = self.chr_last_write_ext;

        if large_sprites_enabled && (currently_reading_backgrounds || (ppu_inactive && wrote_ext_register_last)) {
            let chr_bank = self.chr_ext_banks[extended_bank_index as usize];
            let chr_address = banked_memory_index(self.chr_rom.len(), chr_bank_size as usize, chr_bank, address as usize);
            return self.chr_rom[chr_address];
        } else {
            let chr_bank = self.chr_banks[standard_bank_index as usize];
            let chr_address = banked_memory_index(self.chr_rom.len(), chr_bank_size as usize, chr_bank, address as usize);
            return self.chr_rom[chr_address];
        }
    }

    pub fn read_extended_chr(&self, address: u16) -> u8 {
        let chr_bank_size = 4096;
        let nametable_index = self.last_bg_tile_fetch & 0x3FF;
        let extended_tile_attributes = self.extram[nametable_index as usize];
        let chr_bank = (self.chr_bank_high_bits << 6) | ((extended_tile_attributes as usize) & 0b0011_1111);
        let chr_address = banked_memory_index(self.chr_rom.len(), chr_bank_size as usize, chr_bank, address as usize);
        return self.chr_rom[chr_address];
    }

        pub fn read_extended_attribute(&self) -> u8 {
        let nametable_index = self.last_bg_tile_fetch & 0x3FF;
        let extended_tile_attributes = self.extram[nametable_index as usize];
        let palette_index = (extended_tile_attributes & 0b1100_0000) >> 6;
        // Duplicate the palette four times; this is easier than working out which sub-index the PPU is going to
        // read here. We're overriding every fetch anyway.
        let combined_attribute = palette_index << 6 | palette_index << 4 | palette_index << 2 | palette_index;
        return combined_attribute as u8;
    }

    fn read_pcm_sample(&mut self, address: u16) {
        if self.pcm_read_mode {
            match address {
                0x8000 ..= 0xBFFF => {
                    self.pcm_level = self.read_prg(address);
                },
                _ => {}
            }
        }
    }

    fn _read_cpu(&self, address: u16) -> Option<u8> {
        match address {
            0x5010 => {
                let mut pcm_status = 0;
                if self.pcm_read_mode {
                    pcm_status |= 0b0000_0001;
                }
                if self.pcm_irq_pending {
                    pcm_status |= 0b1000_0000;   
                }
                return Some(pcm_status)
            },
            0x5015 => {
                let mut pulse_status = 0;
                if self.pulse_1.length_counter.length > 0 {
                    pulse_status += 0b0000_0001;
                }
                if self.pulse_2.length_counter.length > 0 {
                    pulse_status += 0b0000_0010;
                }
                return Some(pulse_status);
            },
            0x5204 => {
                let mut status = 0;
                if self.irq_pending {
                    status |= 0b1000_0000;
                }
                if self.in_frame {
                    status |= 0b0100_0000;
                }
                return Some(status);
            }
            0x5C00 ..= 0x5FFF => {
                match self.extended_ram_mode {
                    2 ..= 3 => {return Some(self.extram[address as usize - 0x5C00]);},
                    _ => return None
                }
            }
            0x5205 => {
                let result = self.multiplicand_a as u16 * self.multiplicand_b as u16;
                return Some((result & 0xFF) as u8);
            },
            0x5206 => {
                let result = self.multiplicand_a as u16 * self.multiplicand_b as u16;
                return Some(((result & 0xFF00) >> 8) as u8);
            },
            0x6000 ..= 0xFFFF => {return Some(self.read_prg(address))},
            _ => return None
        }
    }

    fn detect_scanline(&mut self) {
        // Note: we are *currently* processing fetch #1, so we will not yet consider
        // it to have passed.
        self.ppu_fetches_this_scanline = 0;
        self.ppu_read_mode = PpuMode::Backgrounds;
        if self.in_frame {
            self.current_scanline += 1;
            if self.current_scanline == self.irq_scanline_compare {
                self.irq_pending = true;
            }
        } else {
            self.in_frame = true;
            self.current_scanline = 0;
            self.irq_pending = false;
        }
        if self.current_scanline == 241 {
            self.in_frame = false;
            self.irq_pending = false;
            self.current_scanline = 0;
            self.ppu_read_mode = PpuMode::PpuData;
        }
    }

    fn snoop_ppu_read(&mut self, address: u16) {
        self.cpu_cycles_since_last_ppu_read = 0;
        self.ppu_fetches_this_scanline += 1;
        if self.in_frame && self.ppu_fetches_this_scanline >= 127 {
            self.ppu_read_mode = PpuMode::Sprites;
        }
        if self.in_frame && self.ppu_fetches_this_scanline >= 159 {
            self.ppu_read_mode = PpuMode::Backgrounds;
        }
        if self.consecutive_nametable_count == 2 {
            self.detect_scanline();
        }
        if address == self.last_ppu_fetch && address >= 0x2000 && address <= 0x2FFF {
            self.consecutive_nametable_count += 1;
        } else {
            self.consecutive_nametable_count = 0;
        }
        if self.ppu_fetches_this_scanline % 4 == 0 {
            // The LAST byte we fetched was the nametable byte. Hold onto that address,
            // we need to keep track of it for ExRAM attributes.
            self.last_bg_tile_fetch = self.last_ppu_fetch;
        }
        self.last_ppu_fetch = address;
    }

    fn snoop_cpu_read(&mut self, address: u16) {
        if self.cpu_cycles_since_last_ppu_read < 255 {
            self.cpu_cycles_since_last_ppu_read += 1;
        }
        if self.cpu_cycles_since_last_ppu_read == 4 {
            self.in_frame = false;
            self.ppu_read_mode = PpuMode::PpuData;
        }
        if address == 0xFFFA || address == 0xFFFB {
            self.in_frame = false;
            self.irq_pending = false;
            self.current_scanline = 0;
            self.ppu_read_mode = PpuMode::PpuData;
        }

        self.read_pcm_sample(address);

        match address {
            0x5010 => {self.pcm_irq_pending = false;}
            0x5204 => {self.irq_pending = false;}
            _ => {}
        }
    }

    fn is_extended_attribute(&self) -> bool {
        let ppu_rendering_backgrounds = self.ppu_read_mode == PpuMode::Backgrounds;
        let extended_attributes_enabled = self.extended_ram_mode == 1;
        let reading_attribute_byte = (self.ppu_fetches_this_scanline % 4) == 0;
        return ppu_rendering_backgrounds & extended_attributes_enabled & reading_attribute_byte;
    }

    fn is_extended_pattern(&self) -> bool {
        let ppu_rendering_backgrounds = self.ppu_read_mode == PpuMode::Backgrounds;
        let extended_attributes_enabled = self.extended_ram_mode == 1;
        let tile_sub_cycle = self.ppu_fetches_this_scanline % 4;
        let reading_pattern_byte = (tile_sub_cycle == 1) || (tile_sub_cycle == 2);
        return ppu_rendering_backgrounds & extended_attributes_enabled & reading_pattern_byte;
    }

    fn _read_ppu(&self, address: u16) -> Option<u8> {
        match address {
            0x0000 ..= 0x1FFF => {
                if self.is_extended_pattern() {
                    return Some(self.read_extended_chr(address));
                } else {
                    return Some(self.read_banked_chr(address));
                }
            },
            0x2000 ..= 0x3FFF => {
                if self.is_extended_attribute() {
                    return Some(self.read_extended_attribute());
                } else {
                    return Some(self.read_nametable(address));
                }
            },
            _ => return None
        }
    }
}

impl Mapper for Mmc5 {
    fn print_debug_status(&self) {
        println!("======= MMC5 =======");
        println!("PRG ROM: {}k, PRG RAM: {}k, CHR ROM: {}k", self.prg_rom.len() / 1024, self.prg_ram.len() / 1024, self.chr_rom.len() / 1024);
        println!("PRG Mode: {} CHR Mode: {}, ExRAM Mode: {}", self.prg_mode, self.chr_mode, self.extended_ram_mode);
        println!("PRG Banks: A:{} B:{} C:{} D:{} RAM:{}", self.prg_bank_a, self.prg_bank_b, self.prg_bank_c, self.prg_bank_d, self.prg_ram_bank);
        println!("IRQ E:{} P:{} CMP:{} Detected Scanline: {}, PPU Fetches: {}", self.irq_enabled, self.irq_pending, self.irq_scanline_compare, self.current_scanline, self.ppu_fetches_this_scanline);
        let ppu_mode_name = match self.ppu_read_mode {
            PpuMode::Backgrounds => "Backgrounds",
            PpuMode::Sprites => "Sprites",
            PpuMode::PpuData => "Data",
        };
        println!("PPU Detected Read Mode: {}", ppu_mode_name);
        println!("CHR Banks: A:{}, B:{}, C:{}, D:{}, E:{}, F:{}, G:{}, H:{}", self.chr_banks[0], self.chr_banks[1], self.chr_banks[2], self.chr_banks[3], self.chr_banks[4], self.chr_banks[5], self.chr_banks[6], self.chr_banks[7]);
        println!("CHR Ext:   AA:{}, BB:{}, CC:{}, DD:{}", self.chr_ext_banks[0], self.chr_ext_banks[1], self.chr_ext_banks[2], self.chr_ext_banks[3]);
        println!("Nametables: Q1:{}, Q2:{}, Q3:{}, Q4:{}", self.nametable_mapping & 0b0000_0011, (self.nametable_mapping & 0b0000_1100) >> 2, (self.nametable_mapping & 0b0011_0000) >> 4, (self.nametable_mapping & 0b1100_0000) >> 6);
        println!("Monitors: PPUCTRL: 0x{:02X}, PPUMASK: 0x{:02X}", self.ppuctrl_monitor, self.ppumask_monitor);
        println!("====================");
    }

    fn irq_flag(&self) -> bool {
        return self.irq_enabled && self.irq_pending;
    }

    fn mirroring(&self) -> Mirroring {
        return self.mirroring;
    }
    
    fn read_cpu(&mut self, address: u16) -> Option<u8> {
        self.snoop_cpu_read(address);
        return self._read_cpu(address);
    }

    fn debug_read_cpu(&self, address: u16) -> Option<u8> {
        return self._read_cpu(address);
    }

    fn write_cpu(&mut self, address: u16, data: u8) {
        let duty_table = [
            0b1000_0000,
            0b1100_0000,
            0b1111_0000,
            0b0011_1111,
        ];

        match address {
            0x2000 => {self.ppuctrl_monitor = data},
            0x2001 => {self.ppumask_monitor = data},
            0x5000 => {
                let duty_index =      (data & 0b1100_0000) >> 6;
                let length_disable =  (data & 0b0010_0000) != 0;
                let constant_volume = (data & 0b0001_0000) != 0;

                self.pulse_1.duty = duty_table[duty_index as usize];
                self.pulse_1.length_counter.halt_flag = length_disable;
                self.pulse_1.envelope.looping = length_disable;
                self.pulse_1.envelope.enabled = !(constant_volume);
                self.pulse_1.envelope.volume_register = data & 0b0000_1111;
            },
            0x5002 => {
                let period_low = data as u16;
                self.pulse_1.period_initial = (self.pulse_1.period_initial & 0xFF00) | period_low;
            },
            0x5003 => {
                let period_high =  ((data & 0b0000_0111) as u16) << 8;
                let length_index = (data & 0b1111_1000) >> 3;

                self.pulse_1.period_initial = (self.pulse_1.period_initial & 0x00FF) | period_high;
                self.pulse_1.length_counter.set_length(length_index);

                // Start this note
                self.pulse_1.sequence_counter = 0;
                self.pulse_1.envelope.start_flag = true;
            },
            0x5004 => {
                let duty_index =      (data & 0b1100_0000) >> 6;
                let length_disable =  (data & 0b0010_0000) != 0;
                let constant_volume = (data & 0b0001_0000) != 0;

                self.pulse_2.duty = duty_table[duty_index as usize];
                self.pulse_2.length_counter.halt_flag = length_disable;
                self.pulse_2.envelope.looping = length_disable;
                self.pulse_2.envelope.enabled = !(constant_volume);
                self.pulse_2.envelope.volume_register = data & 0b0000_1111;
            },
            0x5006 => {
                let period_low = data as u16;
                self.pulse_2.period_initial = (self.pulse_2.period_initial & 0xFF00) | period_low;
            },
            0x5007 => {
                let period_high =  ((data & 0b0000_0111) as u16) << 8;
                let length_index =  (data & 0b1111_1000) >> 3;

                self.pulse_2.period_initial = (self.pulse_2.period_initial & 0x00FF) | period_high;
                self.pulse_2.length_counter.set_length(length_index);

                // Start this note
                self.pulse_2.sequence_counter = 0;
                self.pulse_2.envelope.start_flag = true;
            },
            0x5010 => {
                self.pcm_read_mode =  (data & 0b0000_0001) != 0;
                self.pcm_irq_enable =  (data & 0b1000_0000) != 0;
            },
            0x5011 => {
                if !(self.pcm_read_mode) {
                    self.pcm_level = data;
                }
            }
            0x4015 => {
                self.pulse_1.length_counter.channel_enabled  = (data & 0b0001) != 0;
                self.pulse_2.length_counter.channel_enabled  = (data & 0b0010) != 0;
              
                if !(self.pulse_1.length_counter.channel_enabled) {
                    self.pulse_1.length_counter.length = 0;
                }
                if !(self.pulse_2.length_counter.channel_enabled) {
                    self.pulse_2.length_counter.length = 0;
                }
            }
            0x5100 => {self.prg_mode = data & 0b0000_0011;},
            0x5101 => {self.chr_mode = data & 0b0000_0011;},
            0x5102 => {self.prg_ram_magic_low  = data & 0b0000_0011;},
            0x5103 => {self.prg_ram_magic_high = data & 0b0000_0011;},
            0x5104 => {self.extended_ram_mode = data & 0b0000_0011;},
            0x5105 => {self.nametable_mapping = data;},
            0x5106 => {self.fill_tile = data;},
            0x5107 => {
                let fill_color = data & 0b0000_0011;
                // For simplicity, go ahead and store the whole attribute byte
                self.fill_attr = (fill_color << 6) | (fill_color << 2) | (fill_color << 4) | (fill_color);
            },
            0x5113 => {self.prg_ram_bank = data & 0b0111_1111;},
            0x5114 => {
                self.prg_bank_a = data & 0b0111_1111;
                self.prg_bank_a_isram = (data & 0b1000_0000) == 0;
            },
            0x5115 => {
                self.prg_bank_b = data & 0b0111_1111;
                self.prg_bank_b_isram = (data & 0b1000_0000) == 0;
            },
            0x5116 => {
                self.prg_bank_c = data & 0b0111_1111;
                self.prg_bank_c_isram = (data & 0b1000_0000) == 0;
            },
            0x5117 => {self.prg_bank_d = data & 0b0111_1111;},
            0x5C00 ..= 0x5FFF => {
                if self.extended_ram_mode == 0 || self.extended_ram_mode == 1 {
                    // Mapped as either a nametable or extended attributes. Can only write while
                    // PPU is currently rendering, otherwise 0 is written.
                    if self.in_frame {
                        self.extram[address as usize - 0x5C00] = data;
                    } else {
                        self.extram[address as usize - 0x5C00] = 0;
                    }
                }
                if self.extended_ram_mode == 2 {
                    // Mapped as ExRAM, unconditional write
                    self.extram[address as usize - 0x5C00] = data;
                }
            },
            0x5120 ..= 0x5127 => {
                self.chr_banks[address as usize - 0x5120] = data as usize + self.chr_bank_high_bits;
                self.chr_last_write_ext = false;
            },
            0x5128 ..= 0x512B => {
                self.chr_ext_banks[address as usize - 0x5128] = data as usize + self.chr_bank_high_bits;
                self.chr_last_write_ext = true;
            },
            0x5130 => {self.chr_bank_high_bits = ((data & 0b0000_0011) as usize) << 8;},
            0x5203 => {self.irq_scanline_compare = data},
            0x5204 => {self.irq_enabled = (data & 0b1000_0000) != 0;},
            0x5205 => {self.multiplicand_a = data;},
            0x5206 => {self.multiplicand_b = data;},
            0x6000 ..= 0xFFFF => {self.write_prg(address, data);},
            _ => {}
        }
    }

    fn debug_read_ppu(&self, address: u16) -> Option<u8> {
        return self._read_ppu(address);
    }

    fn read_ppu(&mut self, address: u16) -> Option<u8> {
        self.snoop_ppu_read(address);
        return self._read_ppu(address);
    }

    fn write_ppu(&mut self, address: u16, data: u8) {
        match address {
            0x2000 ..= 0x3FFF => {self.write_nametable(address, data)},
            _ => {}
        }
    }

    fn clock_cpu(&mut self) {
        self.audio_sequencer_counter += 1;
        if (self.audio_sequencer_counter & 0b1) == 0 {
            self.pulse_1.clock();
            self.pulse_2.clock();
        }
        if self.audio_sequencer_counter >= 7446 {
            self.pulse_1.envelope.clock();
            self.pulse_2.envelope.clock();
            self.pulse_1.length_counter.clock();
            self.pulse_2.length_counter.clock();
            // Note: MMC5 pulse channels don't support sweep. We're borrowing the implementation
            // from the underlying APU, but intentionally not clocking the sweep units.
            self.audio_sequencer_counter = 0;
        }
    }

    fn mix_expansion_audio(&self, nes_sample: f64) -> f64 {
        let pulse_1_output = (self.pulse_1.output() as f64 / 15.0) - 0.5;
        let pulse_2_output = (self.pulse_2.output() as f64 / 15.0) - 0.5;
        let pcm_output = (self.pcm_level as f64 / 256.0) - 0.5;

        return 
            (pulse_1_output + pulse_2_output) * 0.12 + 
            pcm_output * 0.25 + 
            nes_sample;
    }
}

