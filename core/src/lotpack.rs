use std::fs;
use std::path::Path;

use crate::{CoreResult, Error};

#[derive(Clone)]
pub struct Coverage {
    bits: Box<[u64; 1024]>,
}

impl Coverage {
    fn empty() -> Coverage {
        Coverage {
            bits: Box::new([0; 1024]),
        }
    }

    fn set(&mut self, x: usize, y: usize) {
        let i = y * 256 + x;
        self.bits[i / 64] |= 1 << (i % 64);
    }

    pub fn get(&self, x: u16, y: u16) -> bool {
        let i = y as usize * 256 + x as usize;
        i < 65536 && self.bits[i / 64] & (1 << (i % 64)) != 0
    }

    pub fn count(&self) -> u32 {
        self.bits.iter().map(|b| b.count_ones()).sum()
    }

    pub fn merge(&mut self, o: &Coverage) {
        for (a, b) in self.bits.iter_mut().zip(o.bits.iter()) {
            *a |= b;
        }
    }

    pub fn clash(groups: &[Coverage]) -> Coverage {
        let (mut seen, mut twice) = (Coverage::empty(), Coverage::empty());
        for g in groups {
            for i in 0..1024 {
                twice.bits[i] |= seen.bits[i] & g.bits[i];
                seen.bits[i] |= g.bits[i];
            }
        }
        twice
    }

    pub fn runs(&self, limit: usize) -> Vec<[u16; 3]> {
        let mut out = Vec::new();
        for y in 0..256u16 {
            let mut x = 0u16;
            while x < 256 {
                if !self.get(x, y) {
                    x += 1;
                    continue;
                }
                let start = x;
                while x < 256 && self.get(x, y) {
                    x += 1;
                }
                if out.len() == limit {
                    return out;
                }
                out.push([y, start, x - start]);
            }
        }
        out
    }

    pub fn overlap(&self, o: &Coverage) -> u32 {
        self.bits
            .iter()
            .zip(o.bits.iter())
            .map(|(a, b)| (a & b).count_ones())
            .sum()
    }
}

const MAX_LOTPACK: u64 = 128 * 1024 * 1024;

fn bad(path: &Path, what: &str) -> Error {
    Error::Map(format!("{} ({what})", path.display()))
}

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
    path: &'a Path,
}

impl Reader<'_> {
    fn int(&mut self) -> CoreResult<i32> {
        let end = self.at.checked_add(4).filter(|e| *e <= self.data.len());
        let Some(end) = end else {
            return Err(bad(self.path, "se acaba antes de tiempo"));
        };
        let v = i32::from_le_bytes(self.data[self.at..end].try_into().expect("4 bytes"));
        self.at = end;
        Ok(v)
    }
}

pub fn levels(lotheader: &Path) -> CoreResult<(i32, i32)> {
    let data = fs::read(lotheader).map_err(Error::io(lotheader))?;
    let mut r = Reader {
        data: &data,
        at: if data.starts_with(b"LOTH") { 4 } else { 0 },
        path: lotheader,
    };
    let version = r.int()?;
    if !(0..=1).contains(&version) {
        return Err(bad(lotheader, "versión desconocida"));
    }
    let tiles = r.int()?;
    if tiles < 0 {
        return Err(bad(lotheader, "lista de teselas"));
    }
    for _ in 0..tiles {
        let rest = &r.data[r.at..];
        let Some(n) = rest.iter().position(|b| *b == b'\n') else {
            return Err(bad(lotheader, "lista de teselas"));
        };
        r.at += n + 1;
    }
    if version == 0 {
        r.at += 1;
    }
    let (w, h) = (r.int()?, r.int()?);
    if w != 8 || h != 8 {
        return Err(bad(lotheader, "tamaño de trozo"));
    }
    if version == 0 {
        let n = r.int()?;
        Ok((
            0,
            n.checked_sub(1).ok_or_else(|| bad(lotheader, "niveles"))?,
        ))
    } else {
        Ok((r.int()?, r.int()?))
    }
}

pub fn coverage(lotheader: &Path, lotpack: &Path) -> CoreResult<Coverage> {
    let (lo, hi) = levels(lotheader)?;
    let (lo, hi) = (lo.max(-32), hi.min(31));
    let size = fs::metadata(lotpack).map_err(Error::io(lotpack))?.len();
    if size > MAX_LOTPACK {
        return Err(bad(lotpack, "demasiado grande"));
    }
    let data = fs::read(lotpack).map_err(Error::io(lotpack))?;
    let mut r = Reader {
        data: &data,
        at: 0,
        path: lotpack,
    };
    let head = if data.starts_with(b"LOTP") {
        r.at = 4;
        let version = r.int()?;
        if !(0..=1).contains(&version) {
            return Err(bad(lotpack, "versión desconocida"));
        }
        if version >= 1 { 8 } else { 0 }
    } else {
        0
    };
    let mut out = Coverage::empty();
    for lwx in 0..32 {
        for lwy in 0..32 {
            r.at = head + 4 + (lwx * 32 + lwy) * 8;
            let pos = r.int()?;
            if pos < 0 || pos as usize >= data.len() {
                return Err(bad(lotpack, "posición de trozo"));
            }
            r.at = pos as usize;
            let mut skip = 0;
            for _z in lo..=hi {
                for x in 0..8 {
                    for y in 0..8 {
                        if skip > 0 {
                            skip -= 1;
                            continue;
                        }
                        let count = r.int()?;
                        if count == -1 {
                            skip = r.int()?;
                            if skip > 0 {
                                skip -= 1;
                                continue;
                            }
                        }
                        if count > 1 {
                            r.at =
                                r.at.checked_add(4 * count as usize)
                                    .filter(|e| *e <= data.len())
                                    .ok_or_else(|| bad(lotpack, "recuento de teselas"))?;
                            out.set(lwx * 8 + x, lwy * 8 + y);
                        }
                    }
                }
            }
        }
    }
    Ok(out)
}
