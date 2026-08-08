use std::io;

pub struct PipeFd {
    fds: [libc::c_int; 2],
}

impl PipeFd {
    pub fn new() -> io::Result<Self> {
        let mut fds = [0; 2];
        let rc = unsafe { libc::pipe(fds.as_mut_ptr()) };
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { fds })
    }

    pub fn read_end(&self) -> libc::c_int {
        self.fds[0]
    }

    pub fn write_end(&self) -> libc::c_int {
        self.fds[1]
    }
}

impl Drop for PipeFd {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fds[0]);
            libc::close(self.fds[1]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipe_read_write() {
        let p = PipeFd::new().expect("pipe");
        let wfd = p.write_end();
        let rfd = p.read_end();
        let msg = b"hello";
        let nw = unsafe { libc::write(wfd, msg.as_ptr() as *const libc::c_void, msg.len()) };
        assert_eq!(nw as usize, msg.len());
        let mut buf = [0u8; 16];
        let nr = unsafe { libc::read(rfd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        assert_eq!(nr as usize, msg.len());
        assert_eq!(&buf[..nr as usize], msg);
    }
}
