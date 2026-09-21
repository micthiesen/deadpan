#[derive(Debug)] pub enum SourceDecodeError { Native {code:String,message:String}, InvalidConfiguration(&'static str), Io(std::io::Error) }
impl From<std::io::Error> for SourceDecodeError { fn from(v:std::io::Error)->Self {Self::Io(v)} }
impl std::fmt::Display for SourceDecodeError {fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {write!(f,"{self:?}")}}
#[derive(Clone,Copy)] pub struct DecodeControl<'a>{pub timeout:std::time::Duration,pub cancelled:&'a std::sync::atomic::AtomicBool}
mod input { #[derive(Clone,Copy)] pub struct InputLimits {pub max_input_bytes:u64,pub max_packets:u64,pub max_io_bytes_per_call:u64,pub max_packet_bytes:u64,pub max_pixels:u64,pub max_dimension:u32} }
#[path="/Users/michael/Code/deadpan/native/deadpan-source/src/video_codec.rs"] mod video_codec;
#[path="/Users/michael/Code/deadpan/native/deadpan-source/src/matroska_input.rs"] mod matroska_input;
fn main(){let cancelled=std::sync::atomic::AtomicBool::new(false);for arg in std::env::args().skip(1){let f=std::fs::File::open(&arg).unwrap();let result=matroska_input::validate(&f,input::InputLimits{max_input_bytes:1<<30,max_packets:1_000_000,max_io_bytes_per_call:256<<20,max_packet_bytes:16<<20,max_pixels:16777216,max_dimension:8192},DecodeControl{timeout:std::time::Duration::from_secs(10),cancelled:&cancelled});println!("{} {:?}",arg,result);}}
