use thiserror::Error;

#[derive(Debug, Error)]
pub enum CursorReadError {
    #[error("Unexpected end of data")]
    EndOfData,
}

pub struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Cursor { data, pos: 0 }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pos >= self.data.len()
    }

    fn advance(&mut self) -> Result<(), CursorReadError> {
        if self.pos >= self.data.len() {
            return Err(CursorReadError::EndOfData);
        }

        self.pos = self.pos.checked_add(1).ok_or(CursorReadError::EndOfData)?;
        Ok(())
    }

    /// Reads a big-endian u16 from the current position, advancing by 2 bytes if possible.
    pub fn read_u16(&mut self) -> Result<u16, CursorReadError> {
        let b0 = self.read_u8()?;
        let b1 = self.read_u8()?;
        Ok(u16::from_be_bytes([b0, b1]))
    }

    pub fn peek_u8(&self) -> Result<u8, CursorReadError> {
        self.data
            .get(self.pos)
            .copied()
            .ok_or(CursorReadError::EndOfData)
    }

    pub fn read_u8(&mut self) -> Result<u8, CursorReadError> {
        let Ok(b) = self.peek_u8() else {
            return Err(CursorReadError::EndOfData);
        };
        self.advance()?;
        Ok(b)
    }

    pub fn read_n(&mut self, n: usize) -> Result<&'a [u8], CursorReadError> {
        let end = self.pos.saturating_add(n);
        if end <= self.data.len() {
            let s = &self.data[self.pos..end];
            self.pos = end;
            Ok(s)
        } else {
            Err(CursorReadError::EndOfData)
        }
    }

    pub fn set_pos(&mut self, p: usize) {
        self.pos = p.min(self.data.len());
    }

    pub fn pos(&self) -> usize {
        self.pos
    }
}
