#![no_std]

use soroban_sdk::{contracttype, Env, Map};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TickBitmap {
    pub tick_spacing: i32,
    pub initialized_ticks: Map<i32, bool>,
}

impl TickBitmap {
    pub fn new(tick_spacing: i32) -> Self {
        Self {
            tick_spacing,
            initialized_ticks: Map::new(&Env::default()),
        }
    }

    fn validate_tick(&self, tick: i32) -> Result<(), crate::Error> {
        if tick % self.tick_spacing != 0 {
            return Err(crate::Error::TickSpacingMismatch);
        }
        if tick < -887272 || tick > 887272 {
            return Err(crate::Error::InvalidTickRange);
        }
        Ok(())
    }

    pub fn set_tick_initialized(&mut self, tick: i32, initialized: bool) -> Result<(), crate::Error> {
        self.validate_tick(tick)?;
        if initialized {
            self.initialized_ticks.set(tick, true);
        } else {
            self.initialized_ticks.remove(tick);
        }
        Ok(())
    }

    pub fn is_tick_initialized(&self, tick: i32) -> Result<bool, crate::Error> {
        self.validate_tick(tick)?;
        Ok(self.initialized_ticks.get(tick).unwrap_or(false))
    }

    pub fn next_initialized_tick_within_one_word(
        &self,
        tick: i32,
        lte: bool,
        tick_spacing: i32,
    ) -> Result<(bool, i32), crate::Error> {
        if tick < -887272 || tick > 887272 {
            return Err(crate::Error::InvalidTickRange);
        }

        let mut current = tick;
        
        if lte {
            while current >= -887272 {
                if self.initialized_ticks.get(current).unwrap_or(false) {
                    return Ok((true, current));
                }
                current -= tick_spacing;
            }
            Ok((false, -887272))
        } else {
            while current <= 887272 {
                if self.initialized_ticks.get(current).unwrap_or(false) {
                    return Ok((true, current));
                }
                current += tick_spacing;
            }
            Ok((false, 887272))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitmap_basic() {
        let mut bitmap = TickBitmap::new(10);
        assert!(bitmap.set_tick_initialized(0, true).is_ok());
        assert!(bitmap.is_tick_initialized(0).unwrap());
        assert!(!bitmap.is_tick_initialized(10).unwrap());
        assert!(bitmap.set_tick_initialized(10, true).is_ok());
        assert!(bitmap.is_tick_initialized(10).unwrap());
    }

    #[test]
    fn test_bitmap_next_tick() {
        let mut bitmap = TickBitmap::new(10);
        bitmap.set_tick_initialized(-100, true).unwrap();
        bitmap.set_tick_initialized(0, true).unwrap();
        bitmap.set_tick_initialized(100, true).unwrap();
        
        let (found, tick) = bitmap.next_initialized_tick_within_one_word(-50, true, 10).unwrap();
        assert!(found);
        assert_eq!(tick, -100);
        
        let (found, tick) = bitmap.next_initialized_tick_within_one_word(-50, false, 10).unwrap();
        assert!(found);
        assert_eq!(tick, 0);
    }
}