use actix_web::{error::ErrorInternalServerError, web, HttpResponse};
use rockbox_sys as rb;
use serde::Deserialize;

type HandlerResult = actix_web::Result<HttpResponse>;

#[derive(Deserialize)]
pub struct EqBandInput {
    pub cutoff: i32,
    pub q: i32,
    pub gain: i32,
}

#[derive(Deserialize)]
pub struct SetEqBody {
    pub enabled: Option<bool>,
    pub precut: Option<i32>,
    pub bands: Option<Vec<EqBandInput>>,
}

pub async fn set_eq(body: web::Json<SetEqBody>) -> HandlerResult {
    let body = body.into_inner();
    web::block(move || {
        rb::with_kernel_lock(|| {
            if let Some(enabled) = body.enabled {
                unsafe { rb::global_settings.eq_enabled = enabled };
                rb::sound::dsp::eq_enable(enabled);
            }
            if let Some(precut) = body.precut {
                unsafe { rb::global_settings.eq_precut = precut as u32 };
                rb::sound::dsp::set_eq_precut(precut);
            }
            if let Some(bands) = body.bands {
                for (i, band) in bands.iter().enumerate() {
                    if i >= rb::EQ_NUM_BANDS {
                        break;
                    }
                    let setting = rb::EqBandSetting::new(i, band.cutoff, band.q, band.gain);
                    unsafe { rb::global_settings.eq_band_settings[i] = setting };
                    rb::sound::dsp::set_eq_coefs(i as i32, &setting);
                }
            }
        });
    })
    .await
    .map_err(ErrorInternalServerError)?;
    Ok(HttpResponse::NoContent().finish())
}

#[derive(Deserialize)]
pub struct SetCrossfeedBody {
    pub r#type: Option<i32>,
    pub direct_gain: Option<i32>,
    pub cross_gain: Option<i64>,
    pub hf_attenuation: Option<i64>,
    pub hf_cutoff: Option<i64>,
}

pub async fn set_crossfeed(body: web::Json<SetCrossfeedBody>) -> HandlerResult {
    let body = body.into_inner();
    web::block(move || {
        rb::with_kernel_lock(|| {
            if let Some(t) = body.r#type {
                unsafe { rb::global_settings.crossfeed = t };
                rb::sound::dsp::set_crossfeed_type(t);
            }
            if let Some(dg) = body.direct_gain {
                unsafe { rb::global_settings.crossfeed_direct_gain = dg as u32 };
                rb::sound::dsp::set_crossfeed_direct_gain(dg);
            }
            let cg = body.cross_gain;
            let ha = body.hf_attenuation;
            let hc = body.hf_cutoff;
            if cg.is_some() || ha.is_some() || hc.is_some() {
                let cross_gain = cg
                    .unwrap_or_else(|| unsafe { rb::global_settings.crossfeed_cross_gain as i64 });
                let hf_att = ha.unwrap_or_else(|| unsafe {
                    rb::global_settings.crossfeed_hf_attenuation as i64
                });
                let hf_cut =
                    hc.unwrap_or_else(|| unsafe { rb::global_settings.crossfeed_hf_cutoff as i64 });
                if let Some(cg) = body.cross_gain {
                    unsafe { rb::global_settings.crossfeed_cross_gain = cg as u32 };
                }
                if let Some(ha) = body.hf_attenuation {
                    unsafe { rb::global_settings.crossfeed_hf_attenuation = ha as u32 };
                }
                if let Some(hc) = body.hf_cutoff {
                    unsafe { rb::global_settings.crossfeed_hf_cutoff = hc as u32 };
                }
                rb::sound::dsp::set_crossfeed_cross_params(cross_gain, hf_att, hf_cut);
            }
        });
    })
    .await
    .map_err(ErrorInternalServerError)?;
    Ok(HttpResponse::NoContent().finish())
}

#[derive(Deserialize)]
pub struct SetDitheringBody {
    pub enabled: bool,
}

pub async fn set_dithering(body: web::Json<SetDitheringBody>) -> HandlerResult {
    let enabled = body.into_inner().enabled;
    web::block(move || {
        rb::with_kernel_lock(|| {
            unsafe { rb::global_settings.dithering_enabled = enabled };
            rb::sound::dsp::dither_enable(enabled);
        });
    })
    .await
    .map_err(ErrorInternalServerError)?;
    Ok(HttpResponse::NoContent().finish())
}

#[derive(Deserialize)]
pub struct SetAfrBody {
    pub mode: i32,
}

pub async fn set_afr(body: web::Json<SetAfrBody>) -> HandlerResult {
    let mode = body.into_inner().mode;
    web::block(move || {
        rb::with_kernel_lock(|| {
            unsafe { rb::global_settings.afr_enabled = mode };
            rb::sound::dsp::afr_enable(mode);
        });
    })
    .await
    .map_err(ErrorInternalServerError)?;
    Ok(HttpResponse::NoContent().finish())
}

#[derive(Deserialize)]
pub struct SetPbeBody {
    pub mode: Option<i32>,
    pub precut: Option<i32>,
}

pub async fn set_pbe(body: web::Json<SetPbeBody>) -> HandlerResult {
    let body = body.into_inner();
    web::block(move || {
        rb::with_kernel_lock(|| {
            if let Some(mode) = body.mode {
                unsafe { rb::global_settings.pbe = mode };
                rb::sound::dsp::pbe_enable(mode);
            }
            if let Some(precut) = body.precut {
                unsafe { rb::global_settings.pbe_precut = precut };
                rb::sound::dsp::pbe_precut(precut);
            }
        });
    })
    .await
    .map_err(ErrorInternalServerError)?;
    Ok(HttpResponse::NoContent().finish())
}
