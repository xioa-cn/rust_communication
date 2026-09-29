#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Area {
    Coil,
    DiscreteInput,
    HoldingRegister,
    InputRegister,
}

pub(super) struct Address {
    pub area: Area,
    pub offset: u16,
    pub unit_id: Option<u8>,
}

impl Address {
    pub fn parse(text: &str, bit: bool) -> Result<Self, String> {
        let (unit_id, text) = split_unit(text)?;
        let text = text.to_ascii_uppercase();
        let (area, number) = if let Some(number) = text.strip_prefix("DI") {
            (Area::DiscreteInput, number)
        } else if let Some(number) = text.strip_prefix("HR") {
            (Area::HoldingRegister, number)
        } else if let Some(number) = text.strip_prefix("IR") {
            (Area::InputRegister, number)
        } else if let Some(number) = text.strip_prefix('C') {
            (Area::Coil, number)
        } else if bit {
            (Area::Coil, text.as_str())
        } else {
            (Area::HoldingRegister, text.as_str())
        };
        if number.is_empty() || !number.bytes().all(|digit| digit.is_ascii_digit()) {
            return Err(
                "Modbus address must be a decimal offset, C, DI, HR or IR followed by an offset"
                    .into(),
            );
        }
        let offset = number
            .parse::<u16>()
            .map_err(|_| "Modbus address must be in 0..=65535")?;
        if bit != matches!(area, Area::Coil | Area::DiscreteInput) {
            return Err("Modbus bool values require C/DI; numeric values require HR/IR".into());
        }
        Ok(Self {
            area,
            offset,
            unit_id,
        })
    }

    pub fn validate_length(&self, count: usize) -> Result<(), String> {
        if count == 0 || count > 65_536 - usize::from(self.offset) {
            return Err(
                "Modbus quantity must be nonzero and the final address must not exceed 65535"
                    .into(),
            );
        }
        Ok(())
    }

    pub fn read_function(&self) -> u8 {
        match self.area {
            Area::Coil => 1,
            Area::DiscreteInput => 2,
            Area::HoldingRegister => 3,
            Area::InputRegister => 4,
        }
    }

    pub fn validate_write(&self) -> Result<(), String> {
        match self.area {
            Area::Coil | Area::HoldingRegister => Ok(()),
            Area::DiscreteInput | Area::InputRegister => {
                Err("Modbus DI/IR areas are read-only".into())
            }
        }
    }
}

fn split_unit(text: &str) -> Result<(Option<u8>, &str), String> {
    let text = text.trim();
    let Some((prefix, address)) = text.split_once(';') else {
        return Ok((None, text));
    };
    let (name, number) = prefix
        .split_once('=')
        .ok_or("Modbus station prefix must be x=<unit>;address")?;
    let number = number.trim();
    let address = address.trim();
    if !name.trim().eq_ignore_ascii_case("x") || address.is_empty() || address.contains(';') {
        return Err("Modbus station prefix must appear once, as x=<unit>;address".into());
    }
    if number.is_empty() || !number.bytes().all(|digit| digit.is_ascii_digit()) {
        return Err("Modbus station must be an unsigned decimal number".into());
    }
    let unit_id = number
        .parse::<u8>()
        .map_err(|_| "Modbus station exceeds 255")?;
    Ok((Some(unit_id), address))
}
