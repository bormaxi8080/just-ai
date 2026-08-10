use {
  super::{ExecutionError, io_error},
  std::process::{Child, Command},
};

#[cfg(unix)]
pub(super) struct ProcessTree;

#[cfg(unix)]
impl ProcessTree {
  pub(super) fn configure(command: &mut Command) -> Result<Self, ExecutionError> {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
    Ok(Self)
  }

  pub(super) fn attach(&self, _child: &mut Child) -> Result<(), ExecutionError> {
    Ok(())
  }

  pub(super) fn terminate(&self, child: &mut Child) -> Result<(), ExecutionError> {
    let process_group = i32::try_from(child.id())
      .map_err(|_| ExecutionError("child process id exceeds platform range".into()))?;
    // SAFETY: `process_group` is the positive PID returned by `Child::id`; its
    // negation addresses only the isolated group configured before spawning.
    let result = unsafe { libc::kill(-process_group, libc::SIGKILL) };
    if result == 0 {
      return Ok(());
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
      Ok(())
    } else {
      Err(io_error(error))
    }
  }
}

#[cfg(windows)]
pub(super) struct ProcessTree {
  job: std::os::windows::io::OwnedHandle,
}

#[cfg(windows)]
impl ProcessTree {
  pub(super) fn configure(_command: &mut Command) -> Result<Self, ExecutionError> {
    use {
      std::{
        mem,
        os::windows::io::{AsRawHandle, FromRawHandle},
        ptr,
      },
      windows_sys::Win32::System::JobObjects::{
        CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectExtendedLimitInformation, SetInformationJobObject,
      },
    };

    // SAFETY: null attributes and name request a private, non-inheritable job.
    let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
    if handle.is_null() {
      return Err(io_error(std::io::Error::last_os_error()));
    }
    // SAFETY: `handle` is a new owned handle returned by CreateJobObjectW.
    let job = unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(handle) };
    // SAFETY: the Win32 structure is composed of integer and pointer-sized
    // fields for which the all-zero representation is valid.
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { mem::zeroed() };
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    // SAFETY: `job` remains live and `limits` points to the correctly sized
    // structure required for JobObjectExtendedLimitInformation.
    let configured = unsafe {
      SetInformationJobObject(
        job.as_raw_handle(),
        JobObjectExtendedLimitInformation,
        ptr::addr_of!(limits).cast(),
        mem::size_of_val(&limits) as u32,
      )
    };
    if configured == 0 {
      return Err(io_error(std::io::Error::last_os_error()));
    }
    Ok(Self { job })
  }

  pub(super) fn attach(&self, child: &mut Child) -> Result<(), ExecutionError> {
    use {
      std::os::windows::io::AsRawHandle,
      windows_sys::Win32::System::JobObjects::AssignProcessToJobObject,
    };
    // SAFETY: both handles are live for the duration of the call. The child
    // handle has the assignment and termination rights granted by Command.
    let assigned =
      unsafe { AssignProcessToJobObject(self.job.as_raw_handle(), child.as_raw_handle()) };
    if assigned != 0 {
      return Ok(());
    }
    let error = std::io::Error::last_os_error();
    let _ = child.kill();
    let _ = child.wait();
    Err(io_error(error))
  }

  pub(super) fn terminate(&self, _child: &mut Child) -> Result<(), ExecutionError> {
    use {
      std::os::windows::io::AsRawHandle, windows_sys::Win32::System::JobObjects::TerminateJobObject,
    };
    // SAFETY: the owned job handle is live and has termination access.
    let terminated = unsafe { TerminateJobObject(self.job.as_raw_handle(), 1) };
    if terminated == 0 {
      Err(io_error(std::io::Error::last_os_error()))
    } else {
      Ok(())
    }
  }
}

#[cfg(not(any(unix, windows)))]
pub(super) struct ProcessTree;

#[cfg(not(any(unix, windows)))]
impl ProcessTree {
  pub(super) fn configure(_command: &mut Command) -> Result<Self, ExecutionError> {
    Ok(Self)
  }

  pub(super) fn attach(&self, _child: &mut Child) -> Result<(), ExecutionError> {
    Ok(())
  }

  pub(super) fn terminate(&self, child: &mut Child) -> Result<(), ExecutionError> {
    child.kill().map_err(io_error)
  }
}
