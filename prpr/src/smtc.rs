//! Windows 系统媒体传输控件（SMTC）集成。
//!
//! 让 Phira 在游玩时出现在 Windows 系统的媒体控件 / 音量浮层里，
//! 支持显示当前谱面标题、曲师、曲绘，并响应系统的 播放/暂停/上一首/下一首 按钮。

#![cfg(target_os = "windows")]

use std::os::raw::c_void;
use std::sync::mpsc::{channel, Receiver, Sender};

use windows::core::HSTRING;
use windows::Foundation::{EventRegistrationToken, TimeSpan};
use windows::Media::{
    MediaPlaybackStatus, MediaPlaybackType, SystemMediaTransportControls,
    SystemMediaTransportControlsButton, SystemMediaTransportControlsButtonPressedEventArgs,
    SystemMediaTransportControlsTimelineProperties,
};
use windows::Storage::Streams::{
    DataWriter, InMemoryRandomAccessStream, RandomAccessStreamReference,
};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::WinRT::{ISystemMediaTransportControlsInterop, RoGetActivationFactory};

#[derive(Debug, Clone, Copy)]
pub enum SmtcCommand {
    Play,
    Pause,
    Next,
    Previous,
}

extern "system" {
    fn GetForegroundWindow() -> *mut c_void;
}

pub struct SmtcSession {
    smtc: SystemMediaTransportControls,
    event_token: EventRegistrationToken,
    rx: Receiver<SmtcCommand>,
    _tx: Sender<SmtcCommand>,
}

unsafe impl Send for SmtcSession {}
unsafe impl Sync for SmtcSession {}

impl SmtcSession {
    pub fn new(
        title: &str,
        artist: &str,
        cover: &[u8],
        paused: bool,
    ) -> windows::core::Result<Self> {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_null() {
            return Err(windows::core::Error::new(
                windows::core::HRESULT(-2147023436),
                "no foreground window",
            ));
        }
        let hwnd = HWND(hwnd);

        let activatable_id = HSTRING::from("Windows.Media.SystemMediaTransportControls");
        let interop: ISystemMediaTransportControlsInterop =
            unsafe { RoGetActivationFactory(&activatable_id)? };
        let smtc: SystemMediaTransportControls = unsafe { interop.GetForWindow(hwnd)? };

        unsafe {
            smtc.SetIsEnabled(true)?;
            smtc.SetIsPlayEnabled(true)?;
            smtc.SetIsPauseEnabled(true)?;
            smtc.SetIsNextEnabled(true)?;
            smtc.SetIsPreviousEnabled(true)?;
            smtc.SetIsStopEnabled(false)?;
        }

        let status = if paused {
            MediaPlaybackStatus::Paused
        } else {
            MediaPlaybackStatus::Playing
        };
        unsafe { smtc.SetPlaybackStatus(status)? };

        Self::update_media(&smtc, title, artist, cover)?;

        let (tx, rx) = channel::<SmtcCommand>();
        let tx_cb = tx.clone();
        let handler = windows::Foundation::TypedEventHandler::new(
            move |_sender: &Option<SystemMediaTransportControls>,
                  args: &Option<SystemMediaTransportControlsButtonPressedEventArgs>| {
                if let Some(a) = args {
                    let btn = unsafe { a.Button() }.unwrap_or_default();
                    let cmd = match btn {
                        SystemMediaTransportControlsButton::Play => Some(SmtcCommand::Play),
                        SystemMediaTransportControlsButton::Pause => Some(SmtcCommand::Pause),
                        SystemMediaTransportControlsButton::Next => Some(SmtcCommand::Next),
                        SystemMediaTransportControlsButton::Previous => Some(SmtcCommand::Previous),
                        _ => None,
                    };
                    if let Some(c) = cmd {
                        let _ = tx_cb.send(c);
                    }
                }
                Ok(())
            },
        );
        let token = unsafe { smtc.ButtonPressed(&handler)? };

        Ok(Self {
            smtc,
            event_token: token,
            rx,
            _tx: tx,
        })
    }

    fn update_media(
        smtc: &SystemMediaTransportControls,
        title: &str,
        artist: &str,
        cover: &[u8],
    ) -> windows::core::Result<()> {
        unsafe {
            let du = smtc.DisplayUpdater()?;
            du.SetType(MediaPlaybackType::Music)?;
            let props = du.MusicProperties()?;
            props.SetTitle(&HSTRING::from(title))?;
            props.SetArtist(&HSTRING::from(artist))?;

            if !cover.is_empty() {
                let stream = InMemoryRandomAccessStream::new()?;
                let writer = DataWriter::CreateDataWriter(&stream)?;
                writer.WriteBytes(cover)?;
                writer.StoreAsync()?.get()?;
                writer.FlushAsync()?.get()?;
                writer.DetachStream()?;
                let thumb = RandomAccessStreamReference::CreateFromStream(&stream)?;
                du.SetThumbnail(&thumb)?;
            }

            du.Update()?;
        }
        Ok(())
    }

    pub fn try_recv(&self) -> Option<SmtcCommand> {
        match self.rx.try_recv() {
            Ok(c) => Some(c),
            Err(_) => None,
        }
    }

    pub fn set_paused(&self, paused: bool) -> windows::core::Result<()> {
        let s = if paused {
            MediaPlaybackStatus::Paused
        } else {
            MediaPlaybackStatus::Playing
        };
        unsafe { self.smtc.SetPlaybackStatus(s) }
    }

    /// 更新系统媒体控件的时间轴。单位：秒。
    /// start_secs / end_secs / position_secs 都是相对于音乐原点的秒数，
    /// 可以是负数（谱面前置时间）。
    pub fn update_timeline(
        &self,
        start_secs: f64,
        end_secs: f64,
        position_secs: f64,
    ) -> windows::core::Result<()> {
        let props = SystemMediaTransportControlsTimelineProperties::new()?;
        let to_timespan = |secs: f64| TimeSpan { Duration: (secs * 10_000_000.0) as i64 };
        unsafe {
            props.SetStartTime(to_timespan(start_secs))?;
            props.SetEndTime(to_timespan(end_secs))?;
            props.SetPosition(to_timespan(position_secs))?;
            props.SetMinSeekTime(to_timespan(start_secs))?;
            props.SetMaxSeekTime(to_timespan(end_secs))?;
            self.smtc.UpdateTimelineProperties(&props)?;
        }
        Ok(())
    }
}

impl Drop for SmtcSession {
    fn drop(&mut self) {
        unsafe {
            let _ = self.smtc.RemoveButtonPressed(self.event_token);
            let _ = self.smtc.SetIsEnabled(false);
        }
    }
}
