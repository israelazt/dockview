//! Compiling the shaders and a few small Direct3D helpers.

use super::*;

/// # Safety
/// Calls into the D3D compiler with a valid source buffer.
pub(crate) unsafe fn compile(
    source: &str,
    entry: &str,
    target: windows::core::PCSTR,
) -> Result<ID3DBlob> {
    let entry = std::ffi::CString::new(entry).map_err(|_| missing())?;
    let mut code = None;
    let mut errors = None;
    let result = D3DCompile(
        source.as_ptr().cast(),
        source.len(),
        s!("render.hlsl"),
        None,
        None,
        windows::core::PCSTR(entry.as_ptr().cast()),
        target,
        D3DCOMPILE_OPTIMIZATION_LEVEL3,
        0,
        &mut code,
        Some(&mut errors),
    );
    if let Err(e) = result {
        let detail = errors
            .map(|b| String::from_utf8_lossy(blob_bytes(&b)).into_owned())
            .unwrap_or_default();
        return Err(Error::new(e.code(), detail));
    }
    code.ok_or_else(missing)
}

/// # Safety
/// The returned slice borrows the blob's buffer.
pub(crate) unsafe fn blob_bytes(blob: &ID3DBlob) -> &[u8] {
    std::slice::from_raw_parts(blob.GetBufferPointer().cast::<u8>(), blob.GetBufferSize())
}

/// # Safety
/// `device` must be a valid D3D11 device.
pub(super) unsafe fn create_sampler(
    device: &ID3D11Device,
    filter: windows::Win32::Graphics::Direct3D11::D3D11_FILTER,
) -> Result<ID3D11SamplerState> {
    let desc = D3D11_SAMPLER_DESC {
        Filter: filter,
        AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
        AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
        AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
        ComparisonFunc: D3D11_COMPARISON_NEVER,
        MaxLOD: f32::MAX,
        ..Default::default()
    };
    let mut sampler = None;
    device.CreateSamplerState(&desc, Some(&mut sampler))?;
    sampler.ok_or_else(missing)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every shader of the renderer compiles. It needs the D3D compiler only,
    /// no GPU, so it runs everywhere.
    #[test]
    fn every_shader_entry_point_compiles() {
        for (entry, target) in [
            ("vs_main", s!("vs_5_0")),
            ("ps_nv12", s!("ps_5_0")),
            ("ps_nv12_bicubic", s!("ps_5_0")),
            ("ps_yuy2", s!("ps_5_0")),
        ] {
            // SAFETY: a valid source buffer and entry name.
            let result = unsafe { compile(SHADER_SOURCE, entry, target) };
            assert!(result.is_ok(), "{entry}: {:?}", result.err());
        }
    }
}
