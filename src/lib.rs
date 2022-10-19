//! The purpose of this module is to allow a program to dynamically store data inside its own
//! binary. For compatability with Windows, the "write" operation will always be accompanied 
//! by the process exiting. For the windows case, the behind-the-scenes flow for this goes:
//! - Read self, and write own bytes + MAGIC_BYTES + stored bytes to `evil_twin.exe`
//! - Set environment variables:
//!   - `SELF_STORAGE_TWIN_PATH` to current exe path
//!   - `SELF_STORAGE_TWIN_PID` to current pid
//!   - `SELF_STORAGE_STARTUP_MODE` to "UPDATE_ORIG"
//! - Invoke `evil_twin.exe`
//! - When invoked with `SELF_STORAGE_STARTUP_MODE` == "UPDATE_ORIG", `evil_twin.exe` will
//!   - Kill `SELF_STORAGE_TWIN_PID`
//!   - Overwrite file at `SELF_STORAGE_TWIN_PATH` with exact copy of `evil_twin.exe`
//!   - Set environment variables:
//!     - `SELF_STORAGE_TWIN_PATH` = `evil_twin.exe` path
//!     - `SELF_STORAGE_TWIN_PID` = current (evil_twin) pid
//!     - `SELF_STORAGE_STARTUP_MODE` = "KILL_EVIL_TWIN"
//! - When invoked with `SELF_STORAGE_STARTUP_MODE` == "KILL_EVIL_TWIN", the original exe will
//!   - Kill `SELF_STORAGE_TWIN_PID`
//!   - Delete the file at `SELF_STORAGE_TWIN_PATH`
//!   - Exit
//! 
//! In order to ensure the executable behaves as described above, `self_storage_init()` must be 
//! called at the beginning of any program utilizing this library. 
//! 
//! NOTE: The implimentation described above exists to get around Windows' restriction of not
//! allowing the executables of running programs to be modified. For Linux, this implimentation
//! can probably be simplified greatly!
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::env;
use std::process;
use std::path::PathBuf;

// Can't store these bytes in the order they will be used or the first place they
// appear inside the binary will be not at the end, where we are trying to signal 
// the beginning of the stored data.
const MAGIC_BYTES_REV: &[u8; 24] = b"---egarots-fles-nigeb---";  

fn get_magic_bytes() -> Vec<u8> {
    MAGIC_BYTES_REV.iter().map(|b|*b).rev().collect()
}

fn update_orig() {
    println!("Doing update_orig()");
    let orig_path = PathBuf::from( env::var("SELF_STORAGE_TWIN_PATH").unwrap() );
    let orig_pid: u32 = env::var("SELF_STORAGE_TWIN_PID").unwrap().parse().unwrap();
    
    kill_pid(orig_pid);
    fs::copy(env::current_exe().unwrap(), &orig_path).unwrap();

    // Set env vars
    env::set_var("SELF_STORAGE_TWIN_PID", format!("{}", process::id()));
    env::set_var("SELF_STORAGE_TWIN_PATH", env::current_exe().unwrap());
    env::set_var("SELF_STORAGE_STARTUP_MODE", "KILL_EVIL_TWIN");

    process::Command::new(&orig_path).status().unwrap();
}

fn kill_evil_twin() {
    println!("Doing kill_evil_twin()");
    let evil_twin_path = PathBuf::from( env::var("SELF_STORAGE_TWIN_PATH").unwrap() );
    let evil_twin_pid: u32 = env::var("SELF_STORAGE_TWIN_PID").unwrap().parse().unwrap();

    kill_pid(evil_twin_pid);
    fs::remove_file(evil_twin_path).unwrap();
    process::exit(0);
}

#[cfg(windows)]
fn kill_pid(pid: u32) {
    let mut kill_cmd = process::Command::new("taskkill");
    kill_cmd.arg("/F")
        .arg("/PID")
        .arg(format!("{}", pid));
    kill_cmd.output().unwrap();
}

/// Copy bytes from `input` to `output` until the magic byte sequence `seq` is seen. (Bytes in `seq` are not written.) 
/// Note that more bytes will necesarily be read from `input` than are written to `output`. The number of extra
/// bytes read will be at least `seq.len()`, but could be more. Silently copies the entirety of `input` to `output`
/// if `seq` is never seen. Returns number of bytes written on success. 
fn copy_until_seq<R, W>(input: &mut R, output: &mut W, seq: &[u8]) -> io::Result<usize>
where R: Read, W: Write
{
    if seq.len() == 0 {
        return Ok(0);
    }

    let mut write_count = 0;
    let mut seq_pos=0;
    let mut buf = [0u8; 1024];
    let mut done = false;
    while !done {
        let n = input.read(&mut buf)?;
        if n == 0 {
            return Ok(write_count);
        }

        let mut send_to = 0;
        for i in 0..n {
            if buf[i] == seq[seq_pos] {
                seq_pos += 1;
            } else {
                send_to = i;
                seq_pos = 0;
            }

            if seq_pos == seq.len() {
                done = true;
                break;
            }
        }

        if send_to != 0 { 
            write_count += send_to+1;
            output.write_all(&buf[0..send_to+1])?;
        }
    }

    Ok(write_count)
}

pub fn self_storage_init() {
    println!("Doing self_storage_init()");
    match env::var("SELF_STORAGE_STARTUP_MODE") {
        Ok(v) => {
            match v.as_str() {
                "UPDATE_ORIG"    => update_orig(),
                "KILL_EVIL_TWIN" => kill_evil_twin(),
                _ => {}
            }
        },
        _ => {}
    }
}

pub fn set_stored_data_and_exit(data: &[u8]) {
    println!("Doing set_stored_data_and_exit with {} bytes of data.", data.len());
    // Open self and twin files
    let mut self_file = fs::OpenOptions::new().read(true).open( env::current_exe().unwrap() ).unwrap();
    let mut twin_path = env::current_exe().unwrap();
    twin_path.pop();
    twin_path.push("evil_twin.exe");
    let mut twin_file_builder = fs::OpenOptions::new();
    twin_file_builder.write(true).truncate(true);
    let mut twin_file = match twin_file_builder.open(&twin_path) {
        Ok(f) => f,
        Err(_) => twin_file_builder.create_new(true).open(&twin_path).unwrap()
    };

    // Copy self contents up to MAGIC_BYTES + MAGIC_BYTES + data
    copy_until_seq(&mut self_file, &mut twin_file, &get_magic_bytes()).unwrap();
    twin_file.write_all(&get_magic_bytes()).unwrap();
    twin_file.write_all(data).unwrap();
    drop(twin_file);

    println!("Leaving evil_twin.exe behind...");
    
    // Set env vars
    env::set_var("SELF_STORAGE_TWIN_PID", format!("{}", process::id()));
    env::set_var("SELF_STORAGE_TWIN_PATH", env::current_exe().unwrap());
    env::set_var("SELF_STORAGE_STARTUP_MODE", "UPDATE_ORIG");

    // Launch `evil_twin.exe` (program will not return from this call as twin should kill it)
    println!("Launching evil_twin.exe");
    process::Command::new(&twin_path).status().unwrap();
}

pub struct StoredDataReader {
    inner_file: File,
    inner_buf: [u8; 1024],
    inner_buf_filled: usize,
    inner_buf_pos: usize
}

impl Read for StoredDataReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut pos = 0;
        while self.inner_buf_pos < self.inner_buf_filled && pos < buf.len() {
            buf[pos] = self.inner_buf[self.inner_buf_pos];
            pos += 1;
            self.inner_buf_pos += 1;
        }

        if pos > 0 {
            return Ok(pos);
        }

        self.inner_file.read(buf)
    }
}

pub fn get_stored_data() -> io::Result<StoredDataReader> {
    let mut stored_data_reader = StoredDataReader {
        inner_file: fs::OpenOptions::new().read(true).open( env::current_exe()? )?,
        inner_buf: [0u8; 1024],
        inner_buf_filled: 0,
        inner_buf_pos: 0
    };
    let magic_bytes = get_magic_bytes();
    let mut seq_pos = 0;

    let mut done = false;
    while !done {
        stored_data_reader.inner_buf_filled = stored_data_reader.inner_file.read( &mut stored_data_reader.inner_buf )?;
        stored_data_reader.inner_buf_pos = 0;
        if stored_data_reader.inner_buf_filled == 0 {
            return Ok(stored_data_reader)
        }

        for _ in 0..stored_data_reader.inner_buf_filled {
            if stored_data_reader.inner_buf[stored_data_reader.inner_buf_pos] == magic_bytes[seq_pos] {
                seq_pos += 1;
            } else { 
                seq_pos = 0
            }
            stored_data_reader.inner_buf_pos += 1;
            if seq_pos == magic_bytes.len() {
                done = true;
                break
            }
        }
    }

    Ok(stored_data_reader)
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::Cursor;
    use rand::Rng;

    fn get_random_bytes(n: usize) -> Vec<u8> {
        let mut rng = rand::thread_rng();
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            v.push(rng.gen())
        }
        v
    }

    #[test]
    fn test_copy_until_seq_short() {
        let mut input = Cursor::new(b"HelloWorld");
        let mut output = Cursor::new(vec![0u8; 32]);
        let bytes_copied = copy_until_seq(&mut input, &mut output, b"World").unwrap();
        assert_eq!(bytes_copied, 5);
        let output = output.into_inner();
        assert_eq!(&output[..5], b"Hello");
        assert!(&output[5..].into_iter().all(|b| *b == b"\x00"[0]));
    }

    #[test]
    fn test_copy_until_seq_long() {
        let (s, m, e) = (1_000_000, 100, 1_000);
        let start = get_random_bytes(s);
        let mid   = get_random_bytes(m);
        let end   = get_random_bytes(e);

        let mut full = Vec::with_capacity(s+m+e);
        for b in start.iter() { full.push(*b) }
        for b in mid.iter()   { full.push(*b) }
        for b in end.iter()   { full.push(*b) }

        let mut hopefully_start = Cursor::new(Vec::new());
        let mut full = Cursor::new(full);
        copy_until_seq(&mut full, &mut hopefully_start, &mid).unwrap();
        let hopefully_start = hopefully_start.into_inner();
        
        assert_eq!(start.len(), hopefully_start.len());
        assert_eq!(start, hopefully_start);
    }
}