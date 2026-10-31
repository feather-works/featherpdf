use crate::Renderer;
use fearless_simd::{Level, Select, Simd, SimdBase, SimdInto, mask8x32, u8x32, u16x16};
use hayro_interpret::util::x_y_advances;
use hayro_interpret::{FillRule, ImageData, ImageDrawProps, LumaData, Paint, RgbData};
use kurbo::{Affine, Rect};
use pic_scale::{
    ImageSize, ImageStore, ImageStoreMut, PicScaleError, Resampling, ResamplingFunction, Scaler,
};
use std::sync::Arc;
use vello_cpu::peniko::{Compose, Fill, ImageQuality, ImageSampler, Mix};
use vello_cpu::{Image, ImageSource, Mask, PixelMetadata, Pixmap, peniko};

// Previously, we used `CatmullRom`. The problem with that one is that it
// can have negative weights. If we pass a premultiplied buffer to
// `pic-scale`, it can happen that any of the RGB variants end up
// slightly larger than the alpha channel, leading to pixel artifacts when
// rendering. In order to avoid having to do another pass over the buffer
// to clamp, we instead use Hermite, which has similar quality but doesn't
// have this problem.
pub(super) const RESAMPLING_FUNCTION: ResamplingFunction = ResamplingFunction::Hermite;

#[derive(Clone, Copy)]
enum ImagePixelFormat {
    Luma,
    Rgb,
    PremultipliedRgba,
}

struct SolidColorImage {
    color: [u8; 3],
    width: u32,
    height: u32,
    interpolate: bool,
}

enum RenderImageData {
    Rgb(RgbData),
    Luma(LumaData),
    Solid(SolidColorImage),
}

impl From<ImageData> for RenderImageData {
    fn from(value: ImageData) -> Self {
        match value {
            ImageData::Rgb(rgb) => Self::Rgb(rgb),
            ImageData::Luma(luma) => Self::Luma(luma),
        }
    }
}

impl From<RgbData> for RenderImageData {
    fn from(value: RgbData) -> Self {
        Self::Rgb(value)
    }
}

impl RenderImageData {
    fn width(&self) -> u32 {
        match self {
            Self::Rgb(d) => d.width,
            Self::Luma(d) => d.width,
            Self::Solid(d) => d.width,
        }
    }

    fn height(&self) -> u32 {
        match self {
            Self::Rgb(d) => d.height,
            Self::Luma(d) => d.height,
            Self::Solid(d) => d.height,
        }
    }

    fn interpolate(&self) -> bool {
        match self {
            Self::Rgb(d) => d.interpolate,
            Self::Luma(d) => d.interpolate,
            Self::Solid(d) => d.interpolate,
        }
    }
}

impl Renderer<'_> {
    fn draw_image_with_alpha_mask(&mut self, image_data: RenderImageData, alpha_data: LumaData) {
        let img_width = image_data.width();
        let img_height = image_data.height();
        let image_transform = *self.ctx.transform();
        let mask_transform = image_transform
            * Affine::scale_non_uniform(
                img_width as f64 / alpha_data.width as f64,
                img_height as f64 / alpha_data.height as f64,
            );
        let mask_image = SolidColorImage {
            color: [0; 3],
            width: alpha_data.width,
            height: alpha_data.height,
            interpolate: alpha_data.interpolate,
        };

        self.ctx.push_layer(None, None, None, None, None);
        self.ctx.set_transform(image_transform);
        self.draw_image(image_data, None);

        self.ctx.push_layer(
            None,
            Some(peniko::BlendMode::new(Mix::Normal, Compose::DestIn)),
            None,
            None,
            None,
        );
        self.ctx.set_transform(mask_transform);
        // Note that there is a circle between `draw_image` and `draw_image_with_alpha_mask`,
        // but `draw_image_with_alpha_mask` is only called if the dimensions or interpolate
        // values between alpha_data and rgb_data don't match. Here we use a
        // `SolidColorImage` so it doesn't affect it.
        self.draw_image(RenderImageData::Solid(mask_image), Some(alpha_data));
        self.ctx.pop_layer();
        self.ctx.pop_layer();
        self.ctx.set_transform(image_transform);
    }

    fn resize_image_data(
        &self,
        data: Vec<u8>,
        src_width: u32,
        src_height: u32,
        new_width: u32,
        new_height: u32,
        pixel_format: ImagePixelFormat,
    ) -> Vec<u8> {
        match pixel_format {
            ImagePixelFormat::Luma => self.resize_image_data_impl::<1>(
                data,
                src_width,
                src_height,
                new_width,
                new_height,
                |scaler, source_size, target_size| {
                    scaler.plan_planar_resampling(source_size, target_size)
                },
            ),
            ImagePixelFormat::Rgb => self.resize_image_data_impl::<3>(
                data,
                src_width,
                src_height,
                new_width,
                new_height,
                |scaler, source_size, target_size| {
                    scaler.plan_rgb_resampling(source_size, target_size)
                },
            ),
            ImagePixelFormat::PremultipliedRgba => self.resize_image_data_impl::<4>(
                data,
                src_width,
                src_height,
                new_width,
                new_height,
                |scaler, source_size, target_size| {
                    scaler.plan_rgba_resampling(source_size, target_size, false)
                },
            ),
        }
    }

    fn resize_image_data_impl<const N: usize>(
        &self,
        data: Vec<u8>,
        src_width: u32,
        src_height: u32,
        new_width: u32,
        new_height: u32,
        plan: impl FnOnce(
            &Scaler,
            ImageSize,
            ImageSize,
        ) -> Result<Arc<Resampling<u8, N>>, PicScaleError>,
    ) -> Vec<u8> {
        let source_size = ImageSize::new(src_width as usize, src_height as usize);
        let target_size = ImageSize::new(new_width as usize, new_height as usize);
        let src = ImageStore::<u8, N>::from_slice(&data, src_width as usize, src_height as usize)
            .unwrap();
        let mut out = vec![0; new_width as usize * new_height as usize * N];
        let mut dst =
            ImageStoreMut::<u8, N>::from_slice(&mut out, new_width as usize, new_height as usize)
                .unwrap();
        let plan = plan(&self.global.scaler, source_size, target_size).unwrap();
        plan.resample(&src, &mut dst).unwrap();
        out
    }

    fn draw_image(&mut self, image_data: impl Into<RenderImageData>, alpha_data: Option<LumaData>) {
        let image_data = image_data.into();
        let cur_transform = *self.ctx.transform();
        let mut additional_transform = Affine::IDENTITY;

        let (x_scale, y_scale) = {
            let (x, y) = x_y_advances(&cur_transform);
            (x.length() as f32, y.length() as f32)
        };
        let mut img_width = image_data.width();
        let mut img_height = image_data.height();
        let interpolate = image_data.interpolate() || self.global.force_image_interpolation;

        if let Some(a) = &alpha_data
            && (a.width != img_width
                || a.height != img_height
                || (a.interpolate || self.global.force_image_interpolation) != interpolate)
        {
            return self.draw_image_with_alpha_mask(image_data, alpha_data.unwrap());
        }

        let mut quality = if interpolate {
            ImageQuality::Medium
        } else {
            ImageQuality::Low
        };

        let has_alpha = alpha_data.is_some();
        let mut may_have_transparency = has_alpha;
        // FeatherPDF: indo para o cache, reduz só em degraus (metade, um quarto…); o vello faz o resto
        // (menos de 2×). Assim a mesma imagem pronta serve a vários níveis de zoom.
        let (x_scale, y_scale) = if self.guardar_como.is_some() { (degrau(x_scale), degrau(y_scale)) } else { (x_scale, y_scale) };
        let needs_resize = x_scale < 1.0 || y_scale < 1.0;
        let (new_width, new_height) = if needs_resize {
            let w = (img_width as f32 * x_scale)
                .ceil()
                .max(1.0)
                .min((u16::MAX / 2) as f32) as u32;
            let h = (img_height as f32 * y_scale)
                .ceil()
                .max(1.0)
                .min((u16::MAX / 2) as f32) as u32;
            if self.in_type3_glyph {
                quality = ImageQuality::High;
            }
            (w, h)
        } else {
            (img_width, img_height)
        };

        // For luma images without alpha, we can resize as single-channel and
        // expand to RGBA afterwards, which is ~4x faster.
        let mut needs_premultiplication = has_alpha;
        let mut rgba_data = if matches!(&image_data, RenderImageData::Solid(_)) && has_alpha {
            let RenderImageData::Solid(solid) = image_data else {
                unreachable!()
            };
            let alpha = alpha_data.unwrap();

            let alpha_data = if !needs_resize {
                alpha.data
            } else {
                let resized_alpha = self.resize_image_data(
                    alpha.data,
                    img_width,
                    img_height,
                    new_width,
                    new_height,
                    ImagePixelFormat::Luma,
                );
                additional_transform = Affine::scale_non_uniform(
                    img_width as f64 / new_width as f64,
                    img_height as f64 / new_height as f64,
                );
                img_width = new_width;
                img_height = new_height;
                resized_alpha
            };

            let mut out = Vec::with_capacity(img_width as usize * img_height as usize * 4);
            for a in alpha_data {
                out.extend_from_slice(&[solid.color[0], solid.color[1], solid.color[2], a]);
            }
            out
        } else if matches!(&image_data, RenderImageData::Luma(_)) && !has_alpha {
            // We cannot lift this up due to borrowing issues.
            let RenderImageData::Luma(luma) = image_data else {
                unreachable!()
            };

            let luma_data = if !needs_resize {
                luma.data
            } else {
                let resized = self.resize_image_data(
                    luma.data,
                    img_width,
                    img_height,
                    new_width,
                    new_height,
                    ImagePixelFormat::Luma,
                );
                additional_transform = Affine::scale_non_uniform(
                    img_width as f64 / new_width as f64,
                    img_height as f64 / new_height as f64,
                );
                img_width = new_width;
                img_height = new_height;
                resized
            };

            luma_data
                .iter()
                .flat_map(|g| [*g, *g, *g, 255])
                .collect::<Vec<_>>()
        } else if matches!(&image_data, RenderImageData::Luma(_)) && has_alpha {
            let RenderImageData::Luma(luma) = image_data else {
                unreachable!()
            };
            let alpha = alpha_data.unwrap();

            let (luma_data, alpha_data) = if !needs_resize {
                (luma.data, alpha.data)
            } else {
                let resized_luma = self.resize_image_data(
                    luma.data,
                    img_width,
                    img_height,
                    new_width,
                    new_height,
                    ImagePixelFormat::Luma,
                );
                let resized_alpha = self.resize_image_data(
                    alpha.data,
                    img_width,
                    img_height,
                    new_width,
                    new_height,
                    ImagePixelFormat::Luma,
                );
                additional_transform = Affine::scale_non_uniform(
                    img_width as f64 / new_width as f64,
                    img_height as f64 / new_height as f64,
                );
                img_width = new_width;
                img_height = new_height;
                (resized_luma, resized_alpha)
            };

            let mut out = Vec::with_capacity(img_width as usize * img_height as usize * 4);
            for (g, a) in luma_data.iter().zip(alpha_data) {
                out.extend_from_slice(&[*g, *g, *g, a]);
            }
            out
        } else if matches!(&image_data, RenderImageData::Rgb(_)) && needs_resize {
            let RenderImageData::Rgb(rgb) = image_data else {
                unreachable!()
            };

            let resized = self.resize_image_data(
                rgb.data,
                img_width,
                img_height,
                new_width,
                new_height,
                ImagePixelFormat::Rgb,
            );
            let resized_alpha = alpha_data.map(|alpha| {
                self.resize_image_data(
                    alpha.data,
                    img_width,
                    img_height,
                    new_width,
                    new_height,
                    ImagePixelFormat::Luma,
                )
            });
            additional_transform = Affine::scale_non_uniform(
                img_width as f64 / new_width as f64,
                img_height as f64 / new_height as f64,
            );
            img_width = new_width;
            img_height = new_height;

            let mut out = Vec::with_capacity((img_width * img_height) as usize * 4);
            if let Some(alpha) = resized_alpha {
                for (px, a) in resized.chunks_exact(3).zip(alpha) {
                    out.extend_from_slice(&[px[0], px[1], px[2], a]);
                }
            } else {
                for px in resized.chunks_exact(3) {
                    out.extend_from_slice(&[px[0], px[1], px[2], 255]);
                }
            }
            out
        } else {
            let (rgb_data, alpha_data) = match image_data {
                RenderImageData::Rgb(rgb) => (rgb.data, alpha_data.map(|a| a.data)),
                RenderImageData::Luma(luma) => {
                    let rgb = luma
                        .data
                        .iter()
                        .flat_map(|g| [*g, *g, *g])
                        .collect::<Vec<_>>();
                    (rgb, alpha_data.map(|a| a.data))
                }
                RenderImageData::Solid(solid) => {
                    let mut rgb =
                        Vec::with_capacity(solid.width as usize * solid.height as usize * 3);
                    for _ in 0..solid.width as usize * solid.height as usize {
                        rgb.extend_from_slice(&solid.color);
                    }
                    (rgb, alpha_data.map(|a| a.data))
                }
            };

            let mut rgba_data = match alpha_data {
                None => rgb_data
                    .chunks_exact(3)
                    .flat_map(|rgb| [rgb[0], rgb[1], rgb[2], 255])
                    .collect::<Vec<_>>(),
                Some(alpha) => rgb_data
                    .chunks_exact(3)
                    .zip(alpha)
                    .flat_map(|(rgb, a)| [rgb[0], rgb[1], rgb[2], a])
                    .collect::<Vec<_>>(),
            };

            if !needs_resize {
                rgba_data
            } else {
                premultiply_rgba(self.global.level, &mut rgba_data);

                needs_premultiplication = false;
                let resized = self.resize_image_data(
                    rgba_data,
                    img_width,
                    img_height,
                    new_width,
                    new_height,
                    ImagePixelFormat::PremultipliedRgba,
                );
                additional_transform = Affine::scale_non_uniform(
                    img_width as f64 / new_width as f64,
                    img_height as f64 / new_height as f64,
                );
                img_width = new_width;
                img_height = new_height;
                resized
            }
        };

        if needs_premultiplication {
            premultiply_rgba(self.global.level, &mut rgba_data);
        }

        // The problem is that by default, when applying a bilinear or bicubic scaling, we will
        // sample pixels using an extend (pad/reflect/repeat). For glyphs, this is undesirable
        // as the glyphs will look very bold. Therefore, for glyphs it is more desirable to sample
        // a transparent pixel when reaching the border. Thus, we wrap glyphs in a transparent frame
        // of pixel width 2.
        if self.in_type3_glyph {
            let mut padded_image = vec![];
            padded_image.extend(vec![0; (4 * img_width as usize + 16) * 2]);

            for row in rgba_data.chunks_exact(img_width as usize * 4) {
                padded_image.extend([0; 8]);
                padded_image.extend(row);
                padded_image.extend([0; 8]);
            }

            padded_image.extend(vec![0; (4 * img_width as usize + 16) * 2]);
            img_width += 4;
            img_height += 4;
            additional_transform *= Affine::translate((-2.0, -2.0));
            may_have_transparency = true;

            rgba_data = padded_image;
        }

        let pixmap = Arc::new(Pixmap::from_parts(
            rgba_data,
            img_width as u16,
            img_height as u16,
            PixelMetadata {
                may_have_transparency,
                ..Default::default()
            },
        ));
        if let Some((chave, escala)) = self.guardar_como.take() {
            guardar_pronta(chave, (pixmap.clone(), quality, escala * additional_transform));
        }

        self.draw_pixmap(
            pixmap,
            quality,
            cur_transform * additional_transform,
        );
    }

    fn draw_pixmap(&mut self, pixmap: Arc<Pixmap>, quality: ImageQuality, transform: Affine) {
        let (width, height) = (pixmap.width(), pixmap.height());
        let image = Image {
            image: ImageSource::Pixmap(pixmap),
            sampler: ImageSampler {
                x_extend: peniko::Extend::Pad,
                y_extend: peniko::Extend::Pad,
                quality,
                alpha: 1.0,
            },
        };

        self.ctx.set_transform(transform);
        self.ctx.set_paint(image);
        self.ctx
            .fill_rect(&Rect::new(0.0, 0.0, width as f64, height as f64));
    }

    pub(super) fn draw_pdf_image<'a>(
        &mut self,
        image: hayro_interpret::Image<'a, '_>,
        props: ImageDrawProps<'a>,
    ) {
        self.apply_image_props(&props);
        let mut transform = props.transform;
        self.ctx.set_paint_transform(Affine::IDENTITY);
        self.ctx.set_aliasing_threshold(Some(1));

        let (x, y) = x_y_advances(&transform);
        let target_width = (x.length() * image.width() as f64).ceil() as u32;
        let target_height = (y.length() * image.height() as f64).ceil() as u32;

        match image {
            hayro_interpret::Image::Stencil(s) => {
                s.with_stencil(
                    |stencil, paint| {
                        transform *= Affine::scale_non_uniform(
                            stencil.scale_factors.0 as f64,
                            stencil.scale_factors.1 as f64,
                        );

                        match paint {
                            Paint::Color(c) => {
                                let color = c.to_rgba().to_rgba8();
                                let alpha = color[3];

                                let blend_mode = self.ctx.blend_mode();
                                let push_layer =
                                    alpha != 255 || blend_mode != peniko::BlendMode::default();
                                self.ctx.set_transform(transform);
                                if push_layer {
                                    self.ctx.push_layer(
                                        None,
                                        Some(blend_mode),
                                        Some(alpha as f32 / 255.0),
                                        None,
                                        None,
                                    );
                                }
                                let old_rule = *self.ctx.fill_rule();
                                self.ctx.set_fill_rule(Fill::NonZero);

                                self.draw_image(
                                    RenderImageData::Solid(SolidColorImage {
                                        color: [color[0], color[1], color[2]],
                                        width: stencil.width,
                                        height: stencil.height,
                                        interpolate: stencil.interpolate,
                                    }),
                                    Some(stencil),
                                );

                                if push_layer {
                                    self.ctx.pop_layer();
                                }

                                self.ctx.set_fill_rule(old_rule);
                            }
                            Paint::Pattern(_) => {
                                let (width, height) = (self.ctx.width(), self.ctx.height());
                                let stencil_rect = Rect::new(
                                    0.0,
                                    0.0,
                                    stencil.width as f64,
                                    stencil.height as f64,
                                );
                                let mask_pix = {
                                    let rgb_bytes = ImageData::Rgb(RgbData {
                                        data: vec![
                                            255;
                                            stencil.width as usize
                                                * stencil.height as usize
                                                * 3
                                        ],
                                        width: stencil.width,
                                        height: stencil.height,
                                        interpolate: stencil.interpolate,
                                        scale_factors: stencil.scale_factors,
                                    });
                                    let mut ctx = self.child_context(width, height);
                                    let mut sub_renderer = Renderer::new(&mut ctx, self.global);
                                    let mut sub_pix = Pixmap::new(width, height);
                                    sub_renderer.ctx.set_transform(transform);
                                    sub_renderer.draw_image(rgb_bytes, Some(stencil));
                                    sub_renderer.ctx.flush();
                                    let mut resources = vello_cpu::Resources::default();
                                    sub_renderer.ctx.render(&mut sub_pix, &mut resources);
                                    sub_pix
                                };

                                self.ctx.push_layer(
                                    None,
                                    Some(self.ctx.blend_mode()),
                                    None,
                                    Some(Mask::new_luminance(&mask_pix)),
                                    None,
                                );
                                self.ctx.set_transform(transform);

                                let clip_path = self.set_paint(paint, || stencil_rect, false);
                                if let Some(clip_path) = clip_path.as_ref() {
                                    self.push_clip_path_inner(clip_path, FillRule::NonZero);
                                }
                                self.ctx.fill_rect(&stencil_rect);
                                if clip_path.is_some() {
                                    self.ctx.pop_clip();
                                }

                                self.ctx.pop_layer();
                            }
                        };
                    },
                    Some((target_width, target_height)),
                );
            }
            hayro_interpret::Image::Raster(r) => {
                // A chave é o degrau, não o tamanho exato: zoom 3× e 3,5× usam a mesma imagem pronta.
                let (dx, dy) = (degrau(target_width as f32 / r.width() as f32), degrau(target_height as f32 / r.height() as f32));
                let (target_width, target_height) = ((r.width() as f32 * dx).ceil() as u32, (r.height() as f32 * dy).ceil() as u32);
                let chave = chave_da_imagem(&r, target_width, target_height);
                let tela = Rect::new(0.0, 0.0, self.ctx.width() as f64, self.ctx.height() as f64);
                let pronta = pegar_pronta(chave).or_else(|| ler_pedaco(chave, transform, tela));
                if let Some((pixmap, quality, extra)) = pronta {
                    self.draw_pixmap(pixmap, quality, transform * extra);
                } else if let Some((mut image, mut alpha)) = nativa(&r) {
                    let (sx, sy) = image.scale_factors();
                    let mut escala = Affine::scale_non_uniform(sx as f64, sy as f64);
                    // Os degraus são metades: reduz por média de blocos f×f numa passada, antes do draw_image
                    // (que então só converte), em vez do filtro dele.
                    let (ax, ay) = x_y_advances(&(transform * escala));
                    let k = degrau(ax.length().min(ay.length()) as f32).recip().log2().round() as u32;
                    let f = (1usize << k).min(image.width().min(image.height()) as usize).max(1);
                    if f > 1 {
                        let antes = (image.width() as f64, image.height() as f64);
                        image = reduzir_imagem(image, f);
                        alpha = alpha.map(|a| reduzir_luma(a, f));
                        escala *= Affine::scale_non_uniform(antes.0 / image.width() as f64, antes.1 / image.height() as f64);
                    }
                    transform *= escala;
                    self.ctx.set_transform(transform);
                    self.guardar_como = Some((chave, escala));
                    self.draw_image(image, alpha);
                    self.guardar_como = None;
                }
            }
        }

        self.ctx.set_aliasing_threshold(None);
    }
}

/// FeatherPDF: cada pedaço da tela descomprimia, convertia e redimensionava de novo os JPEGs à vista
/// (3763×3296 numa planta): o grosso do tempo no perf. No mesmo zoom o tamanho pedido se repete,
/// então guarda a imagem pronta para o vello. `Affine` = o que falta aplicar sobre o transform da imagem.
pub(crate) type Pronta = (Arc<Pixmap>, ImageQuality, Affine);
pub(crate) static PRONTAS: std::sync::Mutex<Vec<(u64, Pronta)>> = std::sync::Mutex::new(Vec::new());
// Medido na planta A1 com 14 JPEGs de 300 dpi, rolando a 4×: só RAM, 200 MB → 345 MB e 17 ms por trecho;
// 64 MB + o resto comprimido (COMPRIMIDAS) → ~107 MB e ~18 ms.
const TETO_PRONTAS: usize = 64 << 20;

fn chave_da_imagem(r: &hayro_interpret::RasterImage<'_>, largura: u32, altura: u32) -> u64 {
    use std::hash::{Hash, Hasher};
    // Pelo conteúdo, não pelo endereço: o endereço se repete entre documentos.
    let mut h = rustc_hash::FxHasher::default();
    r.stream().raw_data().hash(&mut h);
    r.stream().dict().data().hash(&mut h);
    (largura, altura).hash(&mut h);
    h.finish()
}

/// A menor potência de 2 (1, ½, ¼…) que não fica abaixo de `escala`; ampliar não muda nada (1).
fn degrau(escala: f32) -> f32 {
    if !(escala > 0.0) || escala >= 1.0 { return 1.0; }
    0.5f32.powi((1.0 / escala).log2().floor() as i32)
}

/// Segunda camada do cache: a imagem pronta que sai da primeira (ou que é grande demais para ela) fica
/// na RAM comprimida com LZ4 em blocos de 256×256, sem perda: as plantas têm muito branco e cor lisa,
/// e uma foto de 47 MB vira 1–4 MB. Cada trecho descomprime só os blocos à vista.
struct Comprimida { largura: u16, altura: u16, transparente: bool, qualidade: ImageQuality, extra: Affine, blocos: Vec<Vec<u8>> }
static COMPRIMIDAS: std::sync::Mutex<Vec<(u64, Arc<Comprimida>)>> = std::sync::Mutex::new(Vec::new());
const BLOCO: usize = 256;
// Bytes já comprimidos (a planta A1 inteira, 14 fotos de 300 dpi, cabe em poucas dezenas de MB).
const TETO_COMPRIMIDAS: usize = 96 << 20;

fn comprimir(chave: u64, pronta: &Pronta) {
    if COMPRIMIDAS.lock().map_or(true, |c| c.iter().any(|e| e.0 == chave)) { return; }
    let (w, h) = (pronta.0.width() as usize, pronta.0.height() as usize);
    let dados = pronta.0.data_as_u8_slice();
    let mut blocos = Vec::with_capacity(w.div_ceil(BLOCO) * h.div_ceil(BLOCO));
    let mut bloco = Vec::with_capacity(BLOCO * BLOCO * 4);
    for by in (0..h).step_by(BLOCO) {
        for bx in (0..w).step_by(BLOCO) {
            let bw = BLOCO.min(w - bx);
            bloco.clear();
            for y in by..(by + BLOCO).min(h) { bloco.extend_from_slice(&dados[(y * w + bx) * 4..(y * w + bx + bw) * 4]); }
            blocos.push(lz4_flex::block::compress(&bloco));
        }
    }
    let nova = Arc::new(Comprimida { largura: w as u16, altura: h as u16, transparente: pronta.0.may_have_transparency(),
        qualidade: pronta.1, extra: pronta.2, blocos });
    let Ok(mut cache) = COMPRIMIDAS.lock() else { return };
    cache.push((chave, nova));
    let bytes = |c: &Comprimida| c.blocos.iter().map(Vec::len).sum::<usize>();
    while cache.len() > 1 && cache.iter().map(|(_, c)| bytes(c)).sum::<usize>() > TETO_COMPRIMIDAS { cache.remove(0); }
}

fn comprimida(chave: u64) -> Option<Arc<Comprimida>> {
    let mut cache = COMPRIMIDAS.lock().ok()?;
    let i = cache.iter().position(|e| e.0 == chave)?;
    let item = cache.remove(i);
    let c = item.1.clone();
    cache.push(item);
    Some(c)
}

/// O retângulo `[x0, x1) × [y0, y1)` (múltiplos de BLOCO, salvo na borda) descomprimido num Pixmap.
fn descomprimir(c: &Comprimida, x0: usize, y0: usize, x1: usize, y1: usize) -> Option<Pixmap> {
    let (w, h) = (c.largura as usize, c.altura as usize);
    let colunas = w.div_ceil(BLOCO);
    let (pw, ph) = (x1 - x0, y1 - y0);
    // Linha a linha, sem zerar a saída antes (zerar 10–20 MB custava ~15% do trecho): cada faixa de
    // blocos é descomprimida e as linhas dela entram em ordem.
    let mut saida = Vec::with_capacity(pw * ph * 4);
    let mut faixa: Vec<Vec<u8>> = Vec::new();
    for by in (y0..y1).step_by(BLOCO) {
        let bh = BLOCO.min(h - by);
        let mut larguras = Vec::new();
        for (k, bx) in (x0..x1).step_by(BLOCO).enumerate() {
            let bw = BLOCO.min(w - bx);
            if faixa.len() <= k { faixa.push(vec![0u8; BLOCO * BLOCO * 4]); }
            lz4_flex::block::decompress_into(&c.blocos[(by / BLOCO) * colunas + bx / BLOCO], &mut faixa[k][..bw * bh * 4]).ok()?;
            larguras.push(bw);
        }
        for y in 0..bh {
            for (k, &bw) in larguras.iter().enumerate() { saida.extend_from_slice(&faixa[k][y * bw * 4..(y + 1) * bw * 4]); }
        }
    }
    Some(Pixmap::from_parts(saida, pw as u16, ph as u16, PixelMetadata { may_have_transparency: c.transparente, ..Default::default() }))
}

/// A imagem inteira, se couber na primeira camada (`ate` bytes descomprimida).
fn ler_comprimida(chave: u64, ate: usize) -> Option<Pronta> {
    let c = comprimida(chave)?;
    if c.largura as usize * c.altura as usize * 4 > ate { return None; }
    let pixmap = descomprimir(&c, 0, 0, c.largura as usize, c.altura as usize)?;
    Some((Arc::new(pixmap), c.qualidade, c.extra))
}

/// Só os blocos à vista de uma imagem comprimida: no zoom alto a tela mostra um pedaço pequeno de uma foto de 47 MB.
fn ler_pedaco(chave: u64, transform: Affine, tela: Rect) -> Option<Pronta> {
    let c = comprimida(chave)?;
    let (w, h) = (c.largura as f64, c.altura as f64);
    let vis = (transform * c.extra).inverse().transform_rect_bbox(tela).inflate(8.0, 8.0).intersect(Rect::new(0.0, 0.0, w, h));
    if vis.width() <= 0.0 || vis.height() <= 0.0 { return None; }
    let b = BLOCO as f64;
    let (x0, y0) = ((vis.x0 / b).floor() * b, (vis.y0 / b).floor() * b);
    let (x1, y1) = (((vis.x1 / b).ceil() * b).min(w), ((vis.y1 / b).ceil() * b).min(h));
    let pixmap = descomprimir(&c, x0 as usize, y0 as usize, x1 as usize, y1 as usize)?;
    Some((Arc::new(pixmap), c.qualidade, c.extra * Affine::translate((x0, y0))))
}

/// A imagem no tamanho dela, aberta uma vez só por sessão: o JPEG (~300 ms numa foto de 300 dpi) vira
/// pixels comprimidos em LZ4 (1–4 MB), e cada nível de zoom novo sai deles (descomprimir ~10 ms).
struct Bruta { cor: (bool, u32, u32, bool, (f32, f32), Vec<u8>), alfa: Option<(u32, u32, bool, (f32, f32), Vec<u8>)> }
static BRUTAS: std::sync::Mutex<Vec<(u64, Arc<Bruta>)>> = std::sync::Mutex::new(Vec::new());
const TETO_BRUTAS: usize = 96 << 20;

fn nativa(r: &hayro_interpret::RasterImage<'_>) -> Option<(ImageData, Option<LumaData>)> {
    let chave = chave_da_imagem(r, 0, 0);
    let achada = {
        let mut cache = BRUTAS.lock().ok()?;
        cache.iter().position(|e| e.0 == chave).map(|i| { let e = cache.remove(i); let b = e.1.clone(); cache.push(e); b })
    };
    let bruta = match achada {
        Some(b) => b,
        None => {
            let mut lida = None;
            r.with_rgba(|i, a| lida = Some((i, a)), Some((r.width(), r.height())));
            let (img, alfa) = lida?;
            let cor = match &img {
                ImageData::Rgb(d) => (true, d.width, d.height, d.interpolate, d.scale_factors, lz4_flex::block::compress(&d.data)),
                ImageData::Luma(d) => (false, d.width, d.height, d.interpolate, d.scale_factors, lz4_flex::block::compress(&d.data)),
            };
            let alfa_c = alfa.as_ref().map(|a| (a.width, a.height, a.interpolate, a.scale_factors, lz4_flex::block::compress(&a.data)));
            let b = Arc::new(Bruta { cor, alfa: alfa_c });
            if let Ok(mut cache) = BRUTAS.lock() {
                cache.push((chave, b.clone()));
                let bytes = |b: &Bruta| b.cor.5.len() + b.alfa.as_ref().map_or(0, |a| a.4.len());
                while cache.len() > 1 && cache.iter().map(|(_, b)| bytes(b)).sum::<usize>() > TETO_BRUTAS { cache.remove(0); }
            }
            // Já aberta: usa direto, sem descomprimir o que acabou de comprimir.
            return Some((img, alfa));
        }
    };
    let (rgb, w, h, interpolate, scale_factors, ref dados) = bruta.cor;
    let data = lz4_flex::block::decompress(dados, (w * h) as usize * if rgb { 3 } else { 1 }).ok()?;
    let img = if rgb { ImageData::Rgb(RgbData { data, width: w, height: h, interpolate, scale_factors }) }
        else { ImageData::Luma(LumaData { data, width: w, height: h, interpolate, scale_factors }) };
    let alfa = match &bruta.alfa {
        Some((w, h, interpolate, scale_factors, dados)) => Some(LumaData {
            data: lz4_flex::block::decompress(dados, (w * h) as usize).ok()?, width: *w, height: *h, interpolate: *interpolate, scale_factors: *scale_factors }),
        None => None,
    };
    Some((img, alfa))
}

/// Média de cada bloco `f`×`f` (`N` canais), numa passada só; a sobra ímpar da borda fica de fora.
fn reduzir<const N: usize>(dados: &[u8], w: u32, h: u32, f: usize) -> (Vec<u8>, u32, u32) {
    let (w, w2, h2) = (w as usize, w as usize / f, h as usize / f);
    let mut out = vec![0u8; w2 * h2 * N];
    let mut soma = vec![0u32; w2 * N];
    let div = (f * f) as u32;
    for y2 in 0..h2 {
        soma.fill(0);
        for dy in 0..f {
            let inicio = (y2 * f + dy) * w * N;
            let linha = &dados[inicio..inicio + w2 * f * N];
            for (s, bloco) in soma.chunks_exact_mut(N).zip(linha.chunks_exact(f * N)) {
                for p in bloco.chunks_exact(N) { for c in 0..N { s[c] += p[c] as u32; } }
            }
        }
        for (o, s) in out[y2 * w2 * N..(y2 + 1) * w2 * N].iter_mut().zip(&soma) { *o = ((s + div / 2) / div) as u8; }
    }
    (out, w2 as u32, h2 as u32)
}

fn reduzir_luma(d: LumaData, f: usize) -> LumaData {
    let (data, width, height) = reduzir::<1>(&d.data, d.width, d.height, f);
    LumaData { data, width, height, ..d }
}

fn reduzir_imagem(img: ImageData, f: usize) -> ImageData {
    match img {
        ImageData::Rgb(d) => { let (data, width, height) = reduzir::<3>(&d.data, d.width, d.height, f); ImageData::Rgb(RgbData { data, width, height, ..d }) }
        ImageData::Luma(d) => ImageData::Luma(reduzir_luma(d, f)),
    }
}

/// Achou: a imagem vai para o fim da fila, que é de onde ninguém sai (a primeira é a há mais tempo sem uso).
fn pegar_pronta(chave: u64) -> Option<Pronta> {
    {
        let mut cache = PRONTAS.lock().ok()?;
        if let Some(i) = cache.iter().position(|p| p.0 == chave) {
            let item = cache.remove(i);
            let pronta = item.1.clone();
            cache.push(item);
            return Some(pronta);
        }
    }
    // Grande demais para a primeira camada: fica comprimida e vem em pedaços (ler_pedaco).
    let pronta = ler_comprimida(chave, TETO_PRONTAS / 2)?;
    guardar_pronta(chave, pronta.clone());
    Some(pronta)
}

pub(crate) fn guardar_pronta(chave: u64, pronta: Pronta) {
    let tamanho = |p: &Pronta| p.0.width() as usize * p.0.height() as usize * 4;
    if tamanho(&pronta) > TETO_PRONTAS / 2 { return comprimir(chave, &pronta); }
    let Ok(mut cache) = PRONTAS.lock() else { return };
    cache.push((chave, pronta));
    // A há mais tempo sem uso sai primeiro, para a camada comprimida.
    while cache.iter().map(|(_, p)| tamanho(p)).sum::<usize>() > TETO_PRONTAS {
        let (k, p) = cache.remove(0);
        comprimir(k, &p);
    }
}

trait Splat4thExt {
    fn splat_4th(self) -> Self;
}

impl<S: Simd> Splat4thExt for u8x32<S> {
    #[inline(always)]
    fn splat_4th(self) -> Self {
        [
            self[3], self[3], self[3], self[3], self[7], self[7], self[7], self[7], self[11],
            self[11], self[11], self[11], self[15], self[15], self[15], self[15], self[19],
            self[19], self[19], self[19], self[23], self[23], self[23], self[23], self[27],
            self[27], self[27], self[27], self[31], self[31], self[31], self[31],
        ]
        .simd_into(self.simd)
    }
}

fn premultiply_rgba(level: Level, data: &mut [u8]) {
    let simd_len = data.len() / 32 * 32;
    let (simd_data, tail) = data.split_at_mut(simd_len);

    #[inline(always)]
    fn premultiply_rgba_simd<S: Simd>(simd: S, data: &mut [u8]) {
        let alpha_lanes: mask8x32<S> = [
            0, 0, 0, -1, 0, 0, 0, -1, 0, 0, 0, -1, 0, 0, 0, -1, 0, 0, 0, -1, 0, 0, 0, -1, 0, 0, 0,
            -1, 0, 0, 0, -1,
        ]
        .simd_into(simd);
        for chunk in data.chunks_exact_mut(32) {
            let rgba = u8x32::from_slice(simd, chunk);
            let alphas = rgba.splat_4th();
            let (rgba_low, rgba_high) = simd.widen_u8x32(rgba);
            let (alpha_low, alpha_high) = simd.widen_u8x32(alphas);
            let premultiplied = simd.narrow_u16x16(
                (rgba_low * alpha_low).div_255(),
                (rgba_high * alpha_high).div_255(),
            );
            alpha_lanes.select(rgba, premultiplied).store_slice(chunk);
        }
    }

    fearless_simd::dispatch!(level, simd => premultiply_rgba_simd(simd, simd_data));

    for pixel in tail.chunks_exact_mut(4) {
        let alpha = u16::from(pixel[3]);
        pixel[0] = div_255(u16::from(pixel[0]) * alpha) as u8;
        pixel[1] = div_255(u16::from(pixel[1]) * alpha) as u8;
        pixel[2] = div_255(u16::from(pixel[2]) * alpha) as u8;
    }
}

trait Div255Ext {
    fn div_255(self) -> Self;
}

impl<S: Simd> Div255Ext for u16x16<S> {
    #[inline(always)]
    fn div_255(self) -> Self {
        (self + Self::splat(self.simd, 255)) >> 8
    }
}

#[inline(always)]
const fn div_255(value: u16) -> u16 {
    (value + 255) >> 8
}
