// Copyright 2018-2024 the Deno authors. All rights reserved. MIT license.
use rand::thread_rng;
use rand::RngCore;
use std::io;
use std::os::windows::io::RawHandle;
use winapi::shared::minwindef::DWORD;
use winapi::um::fileapi::CreateFileA;
use winapi::um::fileapi::OPEN_EXISTING;
use winapi::um::handleapi::CloseHandle;
use winapi::um::handleapi::INVALID_HANDLE_VALUE;
use winapi::um::minwinbase::SECURITY_ATTRIBUTES;
use winapi::um::winbase::CreateNamedPipeA;
use winapi::um::winbase::FILE_FLAG_FIRST_PIPE_INSTANCE;
use winapi::um::winbase::FILE_FLAG_OVERLAPPED;
use winapi::um::winbase::PIPE_ACCESS_DUPLEX;
use winapi::um::winbase::PIPE_READMODE_BYTE;
use winapi::um::winbase::PIPE_TYPE_BYTE;
use winapi::um::winnt::FILE_ATTRIBUTE_NORMAL;
use winapi::um::winnt::GENERIC_READ;
use winapi::um::winnt::GENERIC_WRITE;

/// Create a pair of file descriptors for a named pipe with non-inheritable handles. We cannot use
/// the anonymous pipe from `os_pipe` because that does not support OVERLAPPED (aka async) I/O.
///
/// This is the same way that Rust and pretty much everyone else does it.
///
/// For more information, there is an interesting S.O. question that explains the history, as
/// well as offering a complex NTAPI solution if we decide to try to make these pipes truely
/// anonymous: https://stackoverflow.com/questions/60645/overlapped-i-o-on-anonymous-pipe
pub fn create_named_pipe() -> io::Result<(RawHandle, RawHandle)> {
  let pipe_name = format!(
    r#"\\.\pipe\deno_pipe_{:x}_{:x}\0"#,
    std::process::id(),
    thread_rng().next_u64()
  );

  // Create security attributes to make the pipe handles non-inheritable
  let mut security_attributes = SECURITY_ATTRIBUTES {
    nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as DWORD,
    lpSecurityDescriptor: std::ptr::null_mut(),
    bInheritHandle: 0,
  };

  // SAFETY: Create the pipe server with non-inheritable handle
  let server_handle = unsafe {
    CreateNamedPipeA(
      pipe_name.as_ptr() as *const i8,
      PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
      // Read and write bytes, not messages
      PIPE_TYPE_BYTE | PIPE_READMODE_BYTE,
      // The maximum number of instances that can be created for this pipe.
      1,
      // 4kB buffer sizes
      4096,
      4096,
      // "The default time-out value, in milliseconds, if the WaitNamedPipe function specifies NMPWAIT_USE_DEFAULT_WAIT.
      // Each instance of a named pipe must specify the same value. A value of zero will result in a default time-out of
      // 50 milliseconds."
      0,
      &mut security_attributes,
    )
  };

  if server_handle == INVALID_HANDLE_VALUE {
    return Err(io::Error::last_os_error());
  }

  // SAFETY: Create the pipe client with non-inheritable handle
  let client_handle = unsafe {
    CreateFileA(
      pipe_name.as_ptr() as *const i8,
      GENERIC_READ | GENERIC_WRITE | FILE_FLAG_OVERLAPPED,
      0,
      &mut security_attributes,
      OPEN_EXISTING,
      FILE_ATTRIBUTE_NORMAL,
      std::ptr::null_mut(),
    )
  };

  if client_handle == INVALID_HANDLE_VALUE {
    let err = io::Error::last_os_error();
    // SAFETY: Close the handles if we failed
    unsafe {
      CloseHandle(server_handle);
    }
    return Err(err);
  }

  Ok((server_handle, client_handle))
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs::File;
  use std::io::Read;
  use std::io::Write;
  use std::os::windows::io::FromRawHandle;

  #[test]
  fn make_named_pipe() {
    let (server, client) = create_named_pipe().unwrap();
    // SAFETY: For testing
    let mut server = unsafe { File::from_raw_handle(server) };
    // SAFETY: For testing
    let mut client = unsafe { File::from_raw_handle(client) };

    // Write to the server and read from the client
    server.write_all(b"hello").unwrap();
    let mut buf: [u8; 5] = Default::default();
    client.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"hello");
  }
}
