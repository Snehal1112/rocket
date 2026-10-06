use prost::Message;
use prost_reflect::{DynamicMessage, MessageDescriptor};
use tonic::codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder};
use tonic::Status;

/// A tonic codec for protobuf messages known only at runtime. The encoder writes
/// any `DynamicMessage`. The decoder builds messages of one descriptor.
#[derive(Clone)]
pub(crate) struct DynCodec {
    decode: MessageDescriptor,
}

impl DynCodec {
    pub(crate) fn new(decode: MessageDescriptor) -> Self {
        Self { decode }
    }
}

pub(crate) struct DynEncoder;
pub(crate) struct DynDecoder(MessageDescriptor);

impl Codec for DynCodec {
    type Encode = DynamicMessage;
    type Decode = DynamicMessage;
    type Encoder = DynEncoder;
    type Decoder = DynDecoder;

    fn encoder(&mut self) -> DynEncoder {
        DynEncoder
    }

    fn decoder(&mut self) -> DynDecoder {
        DynDecoder(self.decode.clone())
    }
}

impl Encoder for DynEncoder {
    type Item = DynamicMessage;
    type Error = Status;

    fn encode(&mut self, item: DynamicMessage, dst: &mut EncodeBuf<'_>) -> Result<(), Status> {
        item.encode(dst)
            .map_err(|e| Status::internal(format!("could not encode message: {e}")))
    }
}

impl Decoder for DynDecoder {
    type Item = DynamicMessage;
    type Error = Status;

    fn decode(&mut self, src: &mut DecodeBuf<'_>) -> Result<Option<DynamicMessage>, Status> {
        DynamicMessage::decode(self.0.clone(), src)
            .map(Some)
            .map_err(|e| Status::internal(format!("could not decode message: {e}")))
    }
}
