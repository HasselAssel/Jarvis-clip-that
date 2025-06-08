/*use std::collections::VecDeque;
use std::mem::ManuallyDrop;
use std::ops::Deref;
use windows::Win32::Foundation::HANDLE;
use std::pin::Pin;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::thread::JoinHandle;

use ffmpeg_next::codec::{Flags, Parameters};
use ffmpeg_next::encoder::Audio;
use wasapi::{AudioClient, Direction, Handle, WasapiError};
use windows::core::{HRESULT, Interface, IUnknown, Ref};
use windows::Win32::Media::Audio::{ActivateAudioInterfaceAsync, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_LOOPBACK, AUDIOCLIENT_ACTIVATION_PARAMS, AUDIOCLIENT_ACTIVATION_PARAMS_0, AUDIOCLIENT_ACTIVATION_TYPE_DEFAULT, AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK, AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS, IActivateAudioInterfaceAsyncOperation, IActivateAudioInterfaceCompletionHandler, IActivateAudioInterfaceCompletionHandler_Impl, IAudioCaptureClient, IAudioClient, PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE, PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE, VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, WAVEFORMATEX};
use windows::Win32::System::Com::BLOB;
use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0};
use windows::Win32::System::Threading::{CreateEventW, INFINITE, WaitForSingleObject};
use windows::Win32::System::Variant::VT_BLOB;

use crate::capturer::capture::recorder::{AudioParams, BaseParams, Recorder};
use crate::capturer::error::IdkCustomErrorIGuess;
use crate::capturer::ring_buffer::PacketRingBuffer;

const AAC_FRAME_SIZE: usize = 1024;
const SAMPLE_RATE: usize = 48_000;

pub struct _AudioPerProcess<P: PacketRingBuffer + 'static> {
    audio_encoder: Audio,
    ring_buffer: Arc<Mutex<P>>,

    client: ComObj<IAudioClient>,

    frame: ffmpeg_next::util::frame::audio::Audio,
    empty_frame: ffmpeg_next::util::frame::audio::Audio,

    event: MaybeSafeHANDLE,
}

impl<P: PacketRingBuffer + 'static> _AudioPerProcess<P> {
    pub fn new(p_id: u32, include_tree: bool, ring_buffer: Arc<Mutex<P>>) -> std::result::Result<(Self, Parameters), WasapiError> {
        unsafe {
            windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED).unwrap();
        }

        let client = unsafe {
            // Create audio client
            let mut audio_client_activation_params = AUDIOCLIENT_ACTIVATION_PARAMS {
                ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
                Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
                    ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                        TargetProcessId: p_id,
                        ProcessLoopbackMode: if include_tree {
                            PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE
                        } else {
                            PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE
                        },
                    },
                },
            };
            let pinned_params = Pin::new(&mut audio_client_activation_params);

            let raw_prop = PROPVARIANT {
                Anonymous: PROPVARIANT_0 {
                    Anonymous: ManuallyDrop::new(PROPVARIANT_0_0 {
                        vt: VT_BLOB,
                        wReserved1: 0,
                        wReserved2: 0,
                        wReserved3: 0,
                        Anonymous: PROPVARIANT_0_0_0 {
                            blob: BLOB {
                                cbSize: size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
                                pBlobData: pinned_params.get_mut() as *const _ as *mut _,
                            },
                        },
                    }),
                },
            };

            let activation_prop = ManuallyDrop::new(raw_prop);
            let pinned_prop = Pin::new(activation_prop.deref());
            let activation_params = Some(pinned_prop.get_ref() as *const _);

            // Create completion handler
            let setup = Arc::new((Mutex::new(false), Condvar::new()));
            let callback: IActivateAudioInterfaceCompletionHandler =
                Handler::new(setup.clone()).into();

            // Activate audio interface
            let operation = ActivateAudioInterfaceAsync(
                VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
                &IAudioClient::IID,
                activation_params,
                &callback,
            )?;

            // Wait for completion
            let (lock, cvar) = &*setup;
            let mut completed = lock.lock().unwrap();
            while !*completed {
                completed = cvar.wait(completed).unwrap();
            }
            drop(completed);

            // Get audio client and result
            let mut audio_client: Option<IUnknown> = Default::default();
            let mut result: HRESULT = Default::default();
            operation.GetActivateResult(&mut result, &mut audio_client)?;

            // Ensure successful activation
            result.ok()?;
            // always safe to unwrap if result above is checked first
            let audio_client: IAudioClient = audio_client.unwrap().cast()?;

            audio_client
        };

        let desired_format = wasapi::WaveFormat::new(32, 32, &wasapi::SampleType::Float, SAMPLE_RATE, 2, None);
        let buffer_duration = 10_000_000; // 1s buffer in 100ns units
        let mode = wasapi::StreamMode::EventsShared { autoconvert: false, buffer_duration_hns: buffer_duration };

        //client.initialize_client(&desired_format, &Direction::Capture, &mode)?;
        /*unsafe {client.Initialize(
            mode,
            streamflags,
            buffer_duration,
            period,
            wavefmt.as_waveformatex_ref(),
            None,
        )?};*/
        let old_api_format = desired_format.as_waveformatex_ref();
        let format = WAVEFORMATEX {
            wFormatTag: old_api_format.wFormatTag,
            nChannels: old_api_format.nChannels,
            nSamplesPerSec: old_api_format.nSamplesPerSec,
            nAvgBytesPerSec: old_api_format.nAvgBytesPerSec,
            nBlockAlign: old_api_format.nBlockAlign,
            wBitsPerSample: old_api_format.wBitsPerSample,
            cbSize: old_api_format.cbSize,
        };
        unsafe {client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            buffer_duration,
            0,
            &format,
            None,
        )?};

        //let event = MaybeSafeHANDLE(client.set_get_eventhandle()?);
        let event = unsafe { CreateEventW(None, false, false, None).unwrap() };
        unsafe { client.SetEventHandle(event).unwrap() };

        let event = MaybeSafeHANDLE(event);
        let client = ComObj(client);


        let audio_params = AudioParams {
            base_params: BaseParams {
                codec: ffmpeg_next::codec::encoder::find(ffmpeg_next::codec::Id::AAC).ok_or(ffmpeg_next::Error::EncoderNotFound).unwrap(),
                bit_rate: 128_000,
                max_bit_rate: 150_000,
                flags: Flags::GLOBAL_HEADER,
                rate: SAMPLE_RATE as i32,
            },
            channel_layout: ffmpeg_next::util::channel_layout::ChannelLayout::STEREO,
            format: ffmpeg_next::format::Sample::F32(ffmpeg_next::util::format::sample::Type::Planar),
        };

        let codec = audio_params.base_params.codec;
        let ctx = ffmpeg_next::codec::context::Context::new_with_codec(codec);
        let mut enc = ctx.encoder().audio().unwrap();

        enc.set_rate(audio_params.base_params.rate);
        enc.set_channel_layout(audio_params.channel_layout);
        enc.set_format(audio_params.format);
        enc.set_time_base((1, audio_params.base_params.rate));
        enc.set_flags(audio_params.base_params.flags);

        let audio_encoder = enc.open_as(codec).unwrap();

        let par = Parameters::from(&audio_encoder);

        let frame = ffmpeg_next::util::frame::audio::Audio::new(
            audio_encoder.format(),
            AAC_FRAME_SIZE,
            audio_encoder.channel_layout(),
        );
        let mut empty_frame = ffmpeg_next::util::frame::audio::Audio::new(
            audio_encoder.format(),
            AAC_FRAME_SIZE,
            audio_encoder.channel_layout(),
        );
        let buf = vec![0u8; AAC_FRAME_SIZE * empty_frame.format().bytes() * empty_frame.channel_layout().channels() as usize];
        Self::copy_into_frame(&mut empty_frame, buf);


        Ok((Self {
            audio_encoder,
            ring_buffer,
            client,

            frame,
            empty_frame,

            event,
        }, par))
    }
}

impl<P: PacketRingBuffer> Recorder<P> for _AudioPerProcess<P> {
    fn start_capturing(mut self) -> JoinHandle<std::result::Result<(), IdkCustomErrorIGuess>> {
        thread::spawn(move || -> std::result::Result<(), IdkCustomErrorIGuess> {
            unsafe {
                let mut pts_counter: i64 = 0;
                let mut total_buffer = VecDeque::new();

                let capture_client: IAudioCaptureClient = self.client.GetService().unwrap();

                self.client.Start().unwrap();

                let mut freq = 0;
                let mut start_time = 0;
                windows::Win32::System::Performance::QueryPerformanceFrequency(&mut freq)?;
                windows::Win32::System::Performance::QueryPerformanceCounter(&mut start_time)?;

                loop {
                    WaitForSingleObject(self.event.0, INFINITE);
                    println!("HELO2");

                    let mut packet_length = 0;
                    let mut data = std::ptr::null_mut();
                    let mut flags = 0;

                    let mut device_pos = 0;
                    let mut qpc_pos = 0;
                    capture_client.GetBuffer(
                        &mut data,
                        &mut packet_length,
                        &mut flags,
                        Some(&mut device_pos),
                        Some(&mut qpc_pos),
                    )?;

                    if packet_length > 0 {
                        let new_pts = ((qpc_pos - start_time as u64) * 48_000/*self.format.nSamplesPerSec as u64*/ / freq as u64) as i64;
                        let diff = (new_pts - pts_counter).max(0);
                        if diff >= AAC_FRAME_SIZE as _ {
                            println!("ADDED SILENCE!!!");
                            self.flush_and_silence(&mut total_buffer, diff, &mut pts_counter);
                        }


                        let buffer = std::slice::from_raw_parts(
                            data as *const u8,
                            packet_length as usize * 8/*self.format.nBlockAlign as usize*/,
                        );

                        assert_eq!(buffer.len() % 8, 0);
                        println!("{:?}", buffer);
                        //total_buffer.extend_from_slice(buffer);
                        total_buffer.extend(buffer);

                        capture_client.ReleaseBuffer(packet_length)?;
                    }

                    while total_buffer.len() >= AAC_FRAME_SIZE * 8/*self.format.nBlockAlign as usize*/ {
                        //let buffer: Vec<u8> = total_buffer.drain(..1024 * self.format.nBlockAlign as usize).collect();
                        let buffer: Vec<u8> = total_buffer.drain(..AAC_FRAME_SIZE * 8/*self.format.nBlockAlign as usize*/).collect();
                        let sample_frames = AAC_FRAME_SIZE;//packet_length as usize; //buffer.len() / (*format).nBlockAlign as usize;

                        Self::copy_into_frame(&mut self.frame, buffer);
                        self.frame.set_pts(Some(pts_counter));
                        Self::send_frame_and_receive_packets(&self.ring_buffer, &mut self.audio_encoder, &self.frame, AAC_FRAME_SIZE as i64);

                        pts_counter += sample_frames as i64;
                    }
                }
            }
            // Ok(())
        })
    }
    /*fn start_capturing(mut self) -> JoinHandle<std::result::Result<(), IdkCustomErrorIGuess>> {
        thread::spawn(move || -> std::result::Result<(), IdkCustomErrorIGuess> {
            unsafe{ self.client.Start()?; }
            //let capture_client = self.client.get_audiocaptureclient()?;
            let capture_client: IAudioCaptureClient = unsafe{self.client.GetService()?};

            let mut pts_counter: i64 = 0;
            let mut total_buffer = VecDeque::new();

            let mut freq = 0;
            let mut start_time = 0;
            let mut now_time_buf = 0;
            unsafe {
                windows::Win32::System::Performance::QueryPerformanceFrequency(&mut freq)?;
                windows::Win32::System::Performance::QueryPerformanceCounter(&mut start_time)?;
            }

            loop {
                //self.event.wait_for_event(u32::MAX).unwrap();
                unsafe {WaitForSingleObject(self.event.0, INFINITE);}
                println!("EVENT!");

                unsafe { windows::Win32::System::Performance::QueryPerformanceCounter(&mut now_time_buf)?; }
                let new_pts = (now_time_buf - start_time) * SAMPLE_RATE as i64 / freq;
                let diff = (new_pts - pts_counter).max(0);
                if diff >= AAC_FRAME_SIZE as _ {
                    self.flush_and_silence(&mut total_buffer, diff, &mut pts_counter);
                }

                let _flags = capture_client.read_from_device_to_deque(&mut total_buffer).unwrap();

                while total_buffer.len() >= AAC_FRAME_SIZE * self.frame.format().bytes() * self.frame.channel_layout().channels() as usize {
                    let buffer: Vec<u8> = total_buffer.drain(..AAC_FRAME_SIZE * self.frame.format().bytes() * self.frame.channel_layout().channels() as usize).collect();
                    let sample_frames = AAC_FRAME_SIZE;

                    Self::copy_into_frame(&mut self.frame, buffer);
                    self.frame.set_pts(Some(pts_counter));
                    Self::send_frame_and_receive_packets(&self.ring_buffer, &mut self.audio_encoder, &self.frame, AAC_FRAME_SIZE as i64);

                    pts_counter += sample_frames as i64;
                }
            }
        })
    }*/
}

impl<P: PacketRingBuffer> _AudioPerProcess<P> {
    fn flush_and_silence(&mut self, vec_to_be_flushed: &mut VecDeque<u8>, mut frames_of_silence: i64, start_pts: &mut i64) {
        // flush
        //let mut buffer = vec![0; self.frame.format().bytes()];
        let mut buffer = vec![0; AAC_FRAME_SIZE * self.frame.format().bytes() * self.frame.channel_layout().channels() as usize];
        assert!(buffer.len() >= vec_to_be_flushed.len(), "{}, {}", buffer.len(), vec_to_be_flushed.len());
        println!("vec_to_be_flushed.len(): {}", vec_to_be_flushed.len());
        let flushed_pts = (buffer.len() - vec_to_be_flushed.len()) / 8; //self.client.get_mixformat().unwrap().wave_fmt.Format.nBlockAlign as usize;
        assert_eq!(flushed_pts * /*self.client.get_mixformat().unwrap().wave_fmt.Format.nBlockAlign as usize*/8, buffer.len() - vec_to_be_flushed.len());

        let (first, second) = vec_to_be_flushed.as_slices();
        assert_eq!(first.len() + second.len(), vec_to_be_flushed.len());
        buffer[..first.len()].copy_from_slice(first);
        buffer[first.len()..vec_to_be_flushed.len()].copy_from_slice(second);
        frames_of_silence -= flushed_pts as i64;

        Self::copy_into_frame(&mut self.frame, buffer);
        self.frame.set_pts(Some(*start_pts));
        Self::send_frame_and_receive_packets(&self.ring_buffer, &mut self.audio_encoder, &self.frame, AAC_FRAME_SIZE as i64);
        *start_pts += AAC_FRAME_SIZE as i64;

        // empty frames
        let whole_silent_frames = frames_of_silence / AAC_FRAME_SIZE as i64;//frames_of_silence & !(1024-1); // frames_of_silence / 1024 * 1024;

        println!("{}, {}", whole_silent_frames, frames_of_silence);
        for _ in 0..whole_silent_frames {
            self.empty_frame.set_pts(Some(*start_pts));
            Self::send_frame_and_receive_packets(&self.ring_buffer, &mut self.audio_encoder, &self.empty_frame, AAC_FRAME_SIZE as i64);
            *start_pts += AAC_FRAME_SIZE as i64;
        }
        println!("he done!");
    }

    fn copy_into_frame(frame: &mut ffmpeg_next::util::frame::audio::Audio, buffer: Vec<u8>) {
        let linesize = unsafe { (*frame.as_ptr()).linesize[0] as usize };
        let ptr0 = unsafe { (*frame.as_ptr()).extended_data.offset(0).read() };
        let ptr1 = unsafe { (*frame.as_ptr()).extended_data.offset(1).read() };
        // Get mutable slices to the destination planes first
        let left_plane = unsafe { std::slice::from_raw_parts_mut(ptr0, linesize) };
        let right_plane = unsafe { std::slice::from_raw_parts_mut(ptr1, linesize) };

        // Process buffer directly into planes
        for (i, chunk) in buffer.chunks(8).enumerate() {
            if chunk.len() >= 8 {
                let offset = i * 4;
                if offset + 4 > left_plane.len() || offset + 4 > right_plane.len() {
                    panic!("Destination planes too small");
                }
                left_plane[offset..offset + 4].copy_from_slice(&chunk[0..4]);
                right_plane[offset..offset + 4].copy_from_slice(&chunk[4..8]);
            } else {
                panic!("Data not divisible by 8");
            }
        }
    }
}

use windows_core::*;
use crate::com::ComObj;

#[windows::core::implement(IActivateAudioInterfaceCompletionHandler)]
struct Handler(Arc<(Mutex<bool>, Condvar)>);

impl Handler {
    pub fn new(object: Arc<(Mutex<bool>, Condvar)>) -> Handler {
        Handler(object)
    }
}

impl IActivateAudioInterfaceCompletionHandler_Impl for Handler_Impl {
    fn ActivateCompleted(
        &self,
        _activateoperation: Ref<IActivateAudioInterfaceAsyncOperation>,
    ) -> windows::core::Result<()> {
        let (lock, cvar) = &*self.0;
        let mut completed = lock.lock().unwrap();
        *completed = true;
        drop(completed);
        cvar.notify_one();
        Ok(())
    }
}




struct MaybeSafeHANDLE(HANDLE);

unsafe impl Send for MaybeSafeHANDLE {}

unsafe impl Sync for MaybeSafeHANDLE {}

impl std::ops::Deref for MaybeSafeHANDLE {
    type Target = HANDLE;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for MaybeSafeHANDLE {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}*/