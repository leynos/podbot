"""Contain and terminate a supervised Windows process tree with a Job Object.

Python's standard library has no Job Object binding, so the required ``ctypes``
interop is confined to this module. It remains importable cross-platform;
``create()`` loads Windows libraries only when ``os.name == "nt"``.
"""

from __future__ import annotations

import ctypes
import os
import signal
import subprocess
from ctypes import wintypes

_JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x2000
_JOB_OBJECT_EXTENDED_LIMIT_INFORMATION = 9
_JOB_OBJECT_BASIC_ACCOUNTING_INFORMATION = 1


class _BasicLimitInformation(ctypes.Structure):
    _fields_ = [
        ("PerProcessUserTimeLimit", ctypes.c_longlong),
        ("PerJobUserTimeLimit", ctypes.c_longlong),
        ("LimitFlags", wintypes.DWORD),
        ("MinimumWorkingSetSize", ctypes.c_size_t),
        ("MaximumWorkingSetSize", ctypes.c_size_t),
        ("ActiveProcessLimit", wintypes.DWORD),
        ("Affinity", ctypes.c_size_t),
        ("PriorityClass", wintypes.DWORD),
        ("SchedulingClass", wintypes.DWORD),
    ]


class _IoCounters(ctypes.Structure):
    _fields_ = [
        (name, ctypes.c_ulonglong)
        for name in (
            "ReadOperationCount",
            "WriteOperationCount",
            "OtherOperationCount",
            "ReadTransferCount",
            "WriteTransferCount",
            "OtherTransferCount",
        )
    ]


class _ExtendedLimitInformation(ctypes.Structure):
    _fields_ = [
        ("BasicLimitInformation", _BasicLimitInformation),
        ("IoInfo", _IoCounters),
        ("ProcessMemoryLimit", ctypes.c_size_t),
        ("JobMemoryLimit", ctypes.c_size_t),
        ("PeakProcessMemoryUsed", ctypes.c_size_t),
        ("PeakJobMemoryUsed", ctypes.c_size_t),
    ]


class _BasicAccountingInformation(ctypes.Structure):
    _fields_ = [
        ("TotalUserTime", ctypes.c_longlong),
        ("TotalKernelTime", ctypes.c_longlong),
        ("ThisPeriodTotalUserTime", ctypes.c_longlong),
        ("ThisPeriodTotalKernelTime", ctypes.c_longlong),
        ("TotalPageFaultCount", wintypes.DWORD),
        ("TotalProcesses", wintypes.DWORD),
        ("ActiveProcesses", wintypes.DWORD),
        ("TotalTerminatedProcesses", wintypes.DWORD),
    ]


class WindowsProcessJob:
    """Keep a supervised process and all descendants in one killable job."""

    def __init__(self, handle: int, kernel32: ctypes.CDLL, ntdll: ctypes.CDLL) -> None:
        self.handle = handle
        self.kernel32 = kernel32
        self.ntdll = ntdll

    @classmethod
    def create(cls) -> WindowsProcessJob | None:
        """Create a job that terminates all remaining descendants on close."""
        if os.name != "nt":
            return None
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel32.CreateJobObjectW.argtypes = (ctypes.c_void_p, wintypes.LPCWSTR)
        kernel32.CreateJobObjectW.restype = wintypes.HANDLE
        kernel32.SetInformationJobObject.argtypes = (
            wintypes.HANDLE,
            wintypes.INT,
            ctypes.c_void_p,
            wintypes.DWORD,
        )
        kernel32.SetInformationJobObject.restype = wintypes.BOOL
        kernel32.CloseHandle.argtypes = (wintypes.HANDLE,)
        kernel32.CloseHandle.restype = wintypes.BOOL
        handle = kernel32.CreateJobObjectW(None, None)
        if not handle:
            return None
        limits = _ExtendedLimitInformation()
        limits.BasicLimitInformation.LimitFlags = _JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        configured = kernel32.SetInformationJobObject(
            handle,
            _JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
            ctypes.byref(limits),
            ctypes.sizeof(limits),
        )
        if not configured:
            kernel32.CloseHandle(handle)
            return None
        ntdll = ctypes.WinDLL("ntdll")
        ntdll.NtResumeProcess.argtypes = (wintypes.HANDLE,)
        ntdll.NtResumeProcess.restype = ctypes.c_long
        kernel32.AssignProcessToJobObject.argtypes = (
            wintypes.HANDLE,
            wintypes.HANDLE,
        )
        kernel32.AssignProcessToJobObject.restype = wintypes.BOOL
        kernel32.TerminateJobObject.argtypes = (wintypes.HANDLE, wintypes.UINT)
        kernel32.TerminateJobObject.restype = wintypes.BOOL
        kernel32.QueryInformationJobObject.argtypes = (
            wintypes.HANDLE,
            wintypes.INT,
            ctypes.c_void_p,
            wintypes.DWORD,
            ctypes.c_void_p,
        )
        kernel32.QueryInformationJobObject.restype = wintypes.BOOL
        return cls(handle, kernel32, ntdll)

    def assign_and_resume(self, process_handle: int) -> bool:
        """Assign a suspended child, then resume it before it can spawn children."""
        if not self.kernel32.AssignProcessToJobObject(self.handle, process_handle):
            self._resume(process_handle)
            return False
        try:
            self._resume(process_handle)
        except OSError:
            self.kernel32.TerminateJobObject(self.handle, 1)
            raise
        return True

    def _resume(self, process_handle: int) -> None:
        """Resume the process created suspended for race-free job assignment."""
        status = self.ntdll.NtResumeProcess(process_handle)
        if status < 0:
            raise OSError(f"NtResumeProcess failed with NTSTATUS {status:#x}")

    def active_processes(self) -> int | None:
        """Return the number of live job members, or None if querying failed."""
        information = _BasicAccountingInformation()
        if not self.kernel32.QueryInformationJobObject(
            self.handle,
            _JOB_OBJECT_BASIC_ACCOUNTING_INFORMATION,
            ctypes.byref(information),
            ctypes.sizeof(information),
            None,
        ):
            return None
        return information.ActiveProcesses

    def terminate(self) -> bool:
        """Force-stop the job's full process tree."""
        return bool(self.kernel32.TerminateJobObject(self.handle, 1))

    def close(self) -> None:
        """Close the job handle, killing any process that remains in the job."""
        if self.handle:
            self.kernel32.CloseHandle(self.handle)
            self.handle = 0


def signal_windows_process_tree(process_id: int, signum: int) -> bool:
    """Ask Windows to signal a root and its descendants as a fallback."""
    command = ["taskkill", "/PID", str(process_id), "/T"]
    if signum == signal.SIGKILL:
        command.append("/F")
    try:
        result = subprocess.run(
            command,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=5,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return False
    return result.returncode == 0
