use std::io::{Cursor, Write};

use arith::FieldSerde;
use circuit::Circuit;
use config::{BN254ConfigSha2, Config, GKRScheme, MPIConfig};
use ethabi::ParamType;
use expander::Verifier;
use flate2::write::GzDecoder;
use halo2curves::bn256::Fr;
use revm_primitives::Bytes;
use transcript::Proof;

use crate::{
    Precompile, PrecompileError, PrecompileErrors, PrecompileOutput, PrecompileResult,
    PrecompileWithAddress,
};

pub const VERIFY_EXPANDER: PrecompileWithAddress = PrecompileWithAddress(
    crate::u64_to_address(0xff02),
    Precompile::Standard(verify_expander),
);

const GAS: u64 = 7500;

pub fn verify_expander(input: &Bytes, gas_limit: u64) -> PrecompileResult {
    if gas_limit < GAS {
        return Err(PrecompileErrors::Error(PrecompileError::OutOfGas));
    }
    let input = match input[0] {
        0 => input[1..].to_vec(),
        1 => todo!("get data from side chain"),
        _ => {
            return Err(PrecompileErrors::Error(PrecompileError::Other(
                String::from("data type format error"),
            )))
        }
    };

    let input = {
        let mut e = GzDecoder::new(Vec::new());
        e.write_all(&input).map_err(|e| {
            PrecompileErrors::Error(PrecompileError::other(format!(
                "verify expander gzdecode write_all error:{e}"
            )))
        })?;
        e.finish().map_err(|e| {
            PrecompileErrors::Error(PrecompileError::other(format!(
                "verify expander gzdecode finish error:{e}"
            )))
        })?
    };
    let tokens = ethabi::decode(
        &[ParamType::Tuple(vec![
            ParamType::Bytes,
            ParamType::Bytes,
            ParamType::Bytes,
        ])],
        &input,
    )
    .map_err(|e| {
        PrecompileErrors::Error(PrecompileError::other(format!(
            "decode verify expander error:{e}"
        )))
    })?;

    let tokens = tokens
        .first()
        .cloned()
        .and_then(|token| token.into_tuple())
        .ok_or(PrecompileErrors::Error(PrecompileError::other(
            "verify expander id format error",
        )))?;

    let circuit_bytes = tokens
        .first()
        .cloned()
        .and_then(|token| token.into_bytes())
        .ok_or(PrecompileErrors::Error(PrecompileError::other(
            "verify expander circuit format error",
        )))?;

    let witness_bytes = tokens
        .get(1)
        .cloned()
        .and_then(|token| token.into_bytes())
        .ok_or(PrecompileErrors::Error(PrecompileError::other(
            "verify expander witness format error",
        )))?;

    let proof_bytes = tokens
        .get(2)
        .cloned()
        .and_then(|token| token.into_bytes())
        .ok_or(PrecompileErrors::Error(PrecompileError::other(
            "verify expander proof format error",
        )))?;

    let mut circuit =
        Circuit::<BN254ConfigSha2>::load_circuit_bytes(circuit_bytes).map_err(|e| {
            PrecompileErrors::Error(PrecompileError::other(format!(
                "load_circuit_bytes error:{e}"
            )))
        })?;

    circuit.load_witness_bytes(&witness_bytes, false);

    let config = Config::<BN254ConfigSha2>::new(GKRScheme::Vanilla, MPIConfig::new());
    let verifier = Verifier::new(&config);

    let mut cursor = Cursor::new(proof_bytes);
    let proof = Proof::deserialize_from(&mut cursor).map_err(|e| {
        PrecompileErrors::Error(PrecompileError::other(format!("format proof error:{e}")))
    })?;
    let claimed_v = Fr::deserialize_from(&mut cursor).map_err(|e| {
        PrecompileErrors::Error(PrecompileError::other(format!("format claimed error:{e}")))
    })?;
    let public_input = circuit.public_input.clone();
    let bytes = if verifier.verify(&mut circuit, &public_input, &claimed_v, &proof) {
        "y".as_bytes().to_vec()
    } else {
        "n".as_bytes().to_vec()
    };

    Ok(PrecompileOutput::new(GAS, bytes.into()))
}
