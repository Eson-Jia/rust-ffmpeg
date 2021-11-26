extern crate ffmpeg_next as ffmpeg;

use std::env;
use std::collections::LinkedList;
use std::path::Path;
use std::thread;
use std::sync;
use std::sync::RwLock;

use ffmpeg::{codec, encoder, format, log, media, Rational, dict, Error, Packet};

struct OutputInfo<'a> {
    output_ctx: format::context::Output,
    first_frame_mapping: Vec<bool>,
    input_stream_time_bases: &'a Vec<Rational>,
    packet_list: LinkedList<Packet>,
    join_handle: thread::JoinHandle<()>,
    lock: RwLock<LinkedList<Packet>>,
    finished: bool,
}

impl OutputInfo {
    fn new<P: AsRef<Path>>(
        input_ctx: &format::context::Input,
        path: &P,
    ) -> Result<Self, Error> {
        let output_ctx = format::output_as(path, "flv").unwrap();
        let first_frame_mapping = vec![true; input_ctx.nb_streams() as _];
        let input_stream_time_base = vec![Rational::new(0, 1); input_ctx.nb_streams() as _];
        for (stream_index, stream) in output_ctx.streams().enumerate() {
            let medium_type = stream.codec().medium();
            if medium_type != media::Type::Audio ||
                medium_type != media::Type::Video
            {
                continue;
            }
        }
        Ok(Self {
            finished: false
        })
        Result::Err(Error::Bug)
    }
    fn run(&mut self) {
        if self.finished {
            panic!("finished");
        }
        self.join_handle = thread::spawn(move || {
            loop {
                self.lock.read().unwrap();
                let packet = self.packet_list.pop_front().unwrap();
                packet.write_interleaved(&mut self.output_ctx);
            }
            octx.write_trailer().unwrap();
            self.finished = true;
        });
    }
    fn feed_packet(&mut self, packet: Packet) {
        if self.finished {
            panic!("finished");
        }
        self.lock.write().unwrap();
        self.packet_list.push_back(packet);
    }
}


fn main() {
    let input_file = env::args().nth(1).expect("missing input file");
    let output_prefix = env::args().nth(2).expect("missing output file");

    let the_vec = vec![1, 2, 3, 4, 5];

    ffmpeg::init().unwrap();
    log::set_level(log::Level::Trace);

    let mut ictx = format::input(&input_file).unwrap();
    let mut octx = format::output(&output_file).unwrap();

    let mut stream_mapping = vec![0; ictx.nb_streams() as _];
    let mut first_frame_mapping = vec![true; ictx.nb_streams() as _];
    let mut ist_time_bases = vec![Rational(0, 1); ictx.nb_streams() as _];
    let mut ost_index = 0;
    for (ist_index, ist) in ictx.streams().enumerate() {
        let ist_medium = ist.codec().medium();
        if ist_medium != media::Type::Audio
            && ist_medium != media::Type::Video
            && ist_medium != media::Type::Subtitle
        {
            stream_mapping[ist_index] = -1;
            continue;
        }
        stream_mapping[ist_index] = ost_index;
        ist_time_bases[ist_index] = ist.time_base();
        ost_index += 1;
        let mut ost = octx.add_stream(encoder::find(codec::Id::None)).unwrap();
        ost.set_parameters(ist.parameters());
        // We need to set codec_tag to 0 lest we run into incompatible codec tag
        // issues when muxing into a different container format. Unfortunately
        // there's no high level API to do this (yet).
        unsafe {
            (*ost.parameters().as_mut_ptr()).codec_tag = 0;
        }
    }

    octx.set_metadata(ictx.metadata().to_owned());
    octx.write_header_with(dict!()).unwrap();

    for (stream, mut packet) in ictx.packets() {
        let ist_index = stream.index();
        let ost_index = stream_mapping[ist_index];
        if ost_index < 0 {
            continue;
        }
        let ost = octx.stream(ost_index as _).unwrap();
        packet.rescale_ts(ist_time_bases[ist_index], ost.time_base());
        if packet.dts().is_none() || packet.pts().is_none() || packet.dts().unwrap() < 0 || packet.pts().unwrap() < 0 || packet.dts().unwrap() > packet.pts().unwrap() || first_frame_mapping[ist_index] {
            first_frame_mapping[ist_index] = false;
            packet.set_dts(Option::Some(0));
            packet.set_pts(Option::Some(0));
            packet.set_duration(0);
        }
        packet.set_position(-1);
        packet.set_stream(ost_index as _);
        packet.write_interleaved(&mut octx).unwrap();
    }

    octx.write_trailer().unwrap();
}
