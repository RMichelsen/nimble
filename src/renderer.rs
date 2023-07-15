use std::{collections::HashMap, fmt::Debug, slice};

use imgui::{sys::ImVec4, DrawCmd, DrawCmdParams, DrawIdx, DrawVert, FontAtlasTexture, ImColor32};
use imgui_winit_support::winit::{platform::windows::WindowExtWindows, window::Window};
use url::Url;
use windows::{
    core::ComInterface,
    s, w,
    Foundation::Numerics::Matrix3x2,
    Win32::{
        Foundation::{HWND, RECT},
        Graphics::{
            Direct2D::{
                Common::{
                    D2D1_ALPHA_MODE_IGNORE, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_POINT_2F,
                    D2D_RECT_F,
                },
                D2D1CreateFactory, ID2D1Device, ID2D1DeviceContext, ID2D1Factory2,
                D2D1_ANTIALIAS_MODE_ALIASED, D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES1, D2D1_BRUSH_PROPERTIES,
                D2D1_DEVICE_CONTEXT_OPTIONS_NONE, D2D1_DRAW_TEXT_OPTIONS_NONE,
                D2D1_FACTORY_TYPE_SINGLE_THREADED,
            },
            Direct3D::{
                D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST, D3D11_SRV_DIMENSION_TEXTURE2D,
                D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
            },
            Direct3D11::{
                D3D11CreateDevice, ID3D11BlendState, ID3D11Buffer, ID3D11DepthStencilState,
                ID3D11Device, ID3D11DeviceContext, ID3D11InputLayout, ID3D11PixelShader,
                ID3D11RasterizerState, ID3D11Resource, ID3D11SamplerState,
                ID3D11ShaderResourceView, ID3D11VertexShader, D3D11_BIND_CONSTANT_BUFFER,
                D3D11_BIND_INDEX_BUFFER, D3D11_BIND_SHADER_RESOURCE, D3D11_BIND_VERTEX_BUFFER,
                D3D11_BLEND_DESC, D3D11_BLEND_INV_SRC_ALPHA, D3D11_BLEND_ONE, D3D11_BLEND_OP_ADD,
                D3D11_BLEND_SRC_ALPHA, D3D11_BUFFER_DESC, D3D11_COLOR_WRITE_ENABLE_ALL,
                D3D11_COMPARISON_ALWAYS, D3D11_CPU_ACCESS_WRITE, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                D3D11_CREATE_DEVICE_DEBUG, D3D11_CULL_NONE, D3D11_DEPTH_STENCIL_DESC,
                D3D11_FILL_SOLID, D3D11_FILTER_MIN_MAG_MIP_LINEAR, D3D11_INPUT_ELEMENT_DESC,
                D3D11_INPUT_PER_VERTEX_DATA, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_WRITE_DISCARD,
                D3D11_RASTERIZER_DESC, D3D11_RENDER_TARGET_BLEND_DESC, D3D11_SAMPLER_DESC,
                D3D11_SDK_VERSION, D3D11_SHADER_RESOURCE_VIEW_DESC, D3D11_SUBRESOURCE_DATA,
                D3D11_TEXTURE2D_DESC, D3D11_TEXTURE_ADDRESS_WRAP, D3D11_USAGE_DEFAULT,
                D3D11_USAGE_DYNAMIC, D3D11_VIEWPORT,
            },
            DirectWrite::{
                DWriteCreateFactory, IDWriteFactory5, IDWriteTextFormat, IDWriteTextLayout1,
                DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_WEIGHT_NORMAL, DWRITE_HIT_TEST_METRICS, DWRITE_TEXT_RANGE,
                DWRITE_WORD_WRAPPING_NO_WRAP,
            },
            Dxgi::{
                Common::{
                    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R16_UINT, DXGI_FORMAT_R32G32_FLOAT,
                    DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC,
                },
                IDXGIDevice, IDXGIFactory2, IDXGISurface, IDXGISwapChain1, DXGI_SWAP_CHAIN_DESC1,
                DXGI_SWAP_EFFECT_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT,
            },
        },
    },
};

use crate::{
    buffer::{Buffer, BufferMode},
    theme::Theme,
    user_interface::RenderData,
};

#[derive(Clone, Copy, Debug)]
pub enum TextEffectKind {
    ForegroundColor(Color),
}

#[derive(Clone, Copy, Debug)]
pub struct TextEffect {
    pub kind: TextEffectKind,
    pub start: usize,
    pub length: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub r_u8: u8,
    pub g_u8: u8,
    pub b_u8: u8,
}

impl Color {
    pub fn into_imcol(self) -> ImColor32 {
        ImColor32::from_rgb(self.r_u8, self.g_u8, self.b_u8)
    }

    pub fn into_imvec(self, alpha: f32) -> ImVec4 {
        ImVec4::new(self.r, self.g, self.b, alpha)
    }
}

pub struct Renderer {
    pub font_size: (f32, f32),
    pub character_spacing: f32,
    window_size: (f32, f32),
    d3d11_device: ID3D11Device,
    d3d11_device_context: ID3D11DeviceContext,
    d3d11_blend_state: ID3D11BlendState,
    d3d11_rasterizer_state: ID3D11RasterizerState,
    d3d11_depth_stencil_state: ID3D11DepthStencilState,
    d3d11_input_layout: ID3D11InputLayout,
    d3d11_vertex_shader: ID3D11VertexShader,
    d3d11_pixel_shader: ID3D11PixelShader,
    d3d11_vertex_buffer: ID3D11Buffer,
    d3d11_index_buffer: ID3D11Buffer,
    d3d11_constant_buffer: ID3D11Buffer,
    d3d11_font_atlas_texture: ID3D11ShaderResourceView,
    d3d11_texture_sampler_linear: ID3D11SamplerState,
    d2d1_device: ID2D1Device,
    d2d1_device_context: ID2D1DeviceContext,
    dxgi_swap_chain: IDXGISwapChain1,
    text_format: IDWriteTextFormat,
    dwrite_factory: IDWriteFactory5,
}

impl Renderer {
    pub fn new(window: &Window, font_atlas_texture: &FontAtlasTexture) -> Self {
        let window_size = (
            window.inner_size().width as f32,
            window.inner_size().height as f32,
        );

        let mut flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT;
        if cfg!(debug_assertions) {
            flags |= D3D11_CREATE_DEVICE_DEBUG;
        }

        let (d3d11_device, d3d11_device_context) = {
            let mut device = None;
            let mut context = None;
            let feature_levels = [D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0];
            let mut feature_level = D3D_FEATURE_LEVEL_11_1;
            unsafe {
                D3D11CreateDevice(
                    None,
                    D3D_DRIVER_TYPE_HARDWARE,
                    None,
                    flags,
                    Some(&feature_levels),
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    Some(&mut feature_level),
                    Some(&mut context),
                )
                .unwrap();
            }
            (device.unwrap(), context.unwrap())
        };

        let d3d11_blend_state = {
            let desc = D3D11_BLEND_DESC {
                AlphaToCoverageEnable: false.into(),
                IndependentBlendEnable: true.into(),
                RenderTarget: [D3D11_RENDER_TARGET_BLEND_DESC {
                    BlendEnable: true.into(),
                    SrcBlend: D3D11_BLEND_SRC_ALPHA,
                    DestBlend: D3D11_BLEND_INV_SRC_ALPHA,
                    BlendOp: D3D11_BLEND_OP_ADD,
                    SrcBlendAlpha: D3D11_BLEND_ONE,
                    DestBlendAlpha: D3D11_BLEND_INV_SRC_ALPHA,
                    BlendOpAlpha: D3D11_BLEND_OP_ADD,
                    RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
                }; 8],
            };
            let mut state = None;
            unsafe {
                d3d11_device
                    .CreateBlendState(&desc, Some(&mut state))
                    .unwrap();
            }
            state.unwrap()
        };

        let d3d11_rasterizer_state = {
            let desc = D3D11_RASTERIZER_DESC {
                FillMode: D3D11_FILL_SOLID,
                CullMode: D3D11_CULL_NONE,
                DepthClipEnable: true.into(),
                ScissorEnable: true.into(),
                ..Default::default()
            };
            let mut state = None;
            unsafe {
                d3d11_device
                    .CreateRasterizerState(&desc, Some(&mut state))
                    .unwrap();
            }
            state.unwrap()
        };

        let d3d11_depth_stencil_state = {
            let desc = D3D11_DEPTH_STENCIL_DESC {
                DepthEnable: false.into(),
                StencilEnable: false.into(),
                ..Default::default()
            };
            let mut state = None;
            unsafe {
                d3d11_device
                    .CreateDepthStencilState(&desc, Some(&mut state))
                    .unwrap();
            }
            state.unwrap()
        };

        let d3d11_input_layout = {
            let desc = [
                D3D11_INPUT_ELEMENT_DESC {
                    SemanticName: s!("POSITION"),
                    SemanticIndex: 0,
                    Format: DXGI_FORMAT_R32G32_FLOAT,
                    InputSlot: 0,
                    AlignedByteOffset: 0,
                    InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                    InstanceDataStepRate: 0,
                },
                D3D11_INPUT_ELEMENT_DESC {
                    SemanticName: s!("TEXCOORD"),
                    SemanticIndex: 0,
                    Format: DXGI_FORMAT_R32G32_FLOAT,
                    InputSlot: 0,
                    AlignedByteOffset: 8,
                    InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                    InstanceDataStepRate: 0,
                },
                D3D11_INPUT_ELEMENT_DESC {
                    SemanticName: s!("COLOR"),
                    SemanticIndex: 0,
                    Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                    InputSlot: 0,
                    AlignedByteOffset: 16,
                    InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                    InstanceDataStepRate: 0,
                },
            ];
            let mut layout = None;
            unsafe {
                d3d11_device
                    .CreateInputLayout(&desc, &VERTEX_SHADER, Some(&mut layout))
                    .unwrap();
            }
            layout.unwrap()
        };

        let d3d11_vertex_shader = {
            let mut shader = None;
            unsafe {
                d3d11_device
                    .CreateVertexShader(&VERTEX_SHADER, None, Some(&mut shader))
                    .unwrap();
            }
            shader.unwrap()
        };
        let d3d11_pixel_shader = {
            let mut shader = None;
            unsafe {
                d3d11_device
                    .CreatePixelShader(&PIXEL_SHADER, None, Some(&mut shader))
                    .unwrap();
            }
            shader.unwrap()
        };

        let d3d11_vertex_buffer = {
            let desc = D3D11_BUFFER_DESC {
                ByteWidth: 1024 * 4096,
                Usage: D3D11_USAGE_DYNAMIC,
                BindFlags: D3D11_BIND_VERTEX_BUFFER,
                CPUAccessFlags: D3D11_CPU_ACCESS_WRITE,
                ..Default::default()
            };
            let mut buffer = None;
            unsafe {
                d3d11_device
                    .CreateBuffer(&desc, None, Some(&mut buffer))
                    .unwrap();
            }
            buffer.unwrap()
        };

        let d3d11_index_buffer = {
            let desc = D3D11_BUFFER_DESC {
                ByteWidth: 1024 * 4096,
                Usage: D3D11_USAGE_DYNAMIC,
                BindFlags: D3D11_BIND_INDEX_BUFFER,
                CPUAccessFlags: D3D11_CPU_ACCESS_WRITE,
                ..Default::default()
            };
            let mut buffer = None;
            unsafe {
                d3d11_device
                    .CreateBuffer(&desc, None, Some(&mut buffer))
                    .unwrap();
            }
            buffer.unwrap()
        };

        let d3d11_constant_buffer = {
            let desc = D3D11_BUFFER_DESC {
                ByteWidth: std::mem::size_of::<Constants>() as _,
                Usage: D3D11_USAGE_DYNAMIC,
                BindFlags: D3D11_BIND_CONSTANT_BUFFER,
                CPUAccessFlags: D3D11_CPU_ACCESS_WRITE,
                ..Default::default()
            };
            let mut buffer = None;
            unsafe {
                d3d11_device
                    .CreateBuffer(&desc, None, Some(&mut buffer))
                    .unwrap();
            }
            buffer.unwrap()
        };

        let sub_resource = D3D11_SUBRESOURCE_DATA {
            pSysMem: font_atlas_texture.data.as_ptr().cast(),
            SysMemPitch: font_atlas_texture.width * 4,
            SysMemSlicePitch: 0,
        };

        let desc = D3D11_TEXTURE2D_DESC {
            Width: font_atlas_texture.width,
            Height: font_atlas_texture.height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_SHADER_RESOURCE,
            ..Default::default()
        };

        let texture = {
            let mut texture = None;
            unsafe {
                d3d11_device
                    .CreateTexture2D(&desc, Some(&sub_resource), Some(&mut texture))
                    .unwrap();
            }
            texture.unwrap()
        };

        let d3d11_font_atlas_texture = {
            let mut desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                ViewDimension: D3D11_SRV_DIMENSION_TEXTURE2D,
                ..Default::default()
            };
            desc.Anonymous.Texture2D.MipLevels = 1;
            desc.Anonymous.Texture2D.MostDetailedMip = 0;
            let mut srv = None;
            unsafe {
                d3d11_device
                    .CreateShaderResourceView(&texture, Some(&desc), Some(&mut srv))
                    .unwrap();
            }
            srv.unwrap()
        };

        let d3d11_texture_sampler_linear = {
            let desc = D3D11_SAMPLER_DESC {
                Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
                AddressU: D3D11_TEXTURE_ADDRESS_WRAP,
                AddressV: D3D11_TEXTURE_ADDRESS_WRAP,
                AddressW: D3D11_TEXTURE_ADDRESS_WRAP,
                ComparisonFunc: D3D11_COMPARISON_ALWAYS,
                MaxAnisotropy: 16,
                ..Default::default()
            };
            let mut state = None;
            unsafe {
                d3d11_device
                    .CreateSamplerState(&desc, Some(&mut state))
                    .unwrap()
            }
            state.unwrap()
        };

        let dxgi_device = d3d11_device.cast::<IDXGIDevice>().unwrap();
        let dxgi_factory: IDXGIFactory2 =
            unsafe { dxgi_device.GetAdapter().unwrap().GetParent().unwrap() };

        let d2d1_factory: ID2D1Factory2 =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).unwrap() };
        let d2d1_device = unsafe { d2d1_factory.CreateDevice(&dxgi_device).unwrap() };
        let d2d1_device_context = unsafe {
            d2d1_device
                .CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)
                .unwrap()
        };

        let swap_chain_desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: 0,
            Height: 0,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            SwapEffect: DXGI_SWAP_EFFECT_DISCARD,
            ..Default::default()
        };

        let dxgi_swap_chain = unsafe {
            dxgi_factory
                .CreateSwapChainForHwnd(
                    &d3d11_device,
                    HWND(window.hwnd()),
                    &swap_chain_desc,
                    None,
                    None,
                )
                .unwrap()
        };

        let d2d1_back_buffer: IDXGISurface = unsafe { dxgi_swap_chain.GetBuffer(0).unwrap() };
        let bitmap = unsafe {
            d2d1_device_context
                .CreateBitmapFromDxgiSurface(
                    &d2d1_back_buffer,
                    Some(&D2D1_BITMAP_PROPERTIES1 {
                        pixelFormat: D2D1_PIXEL_FORMAT {
                            format: DXGI_FORMAT_B8G8R8A8_UNORM,
                            alphaMode: D2D1_ALPHA_MODE_IGNORE,
                        },
                        bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                        ..Default::default()
                    }),
                )
                .unwrap()
        };
        unsafe {
            d2d1_device_context.SetTarget(&bitmap);
        }

        let dwrite_factory: IDWriteFactory5 =
            unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).unwrap() };

        let text_format = unsafe {
            dwrite_factory
                .CreateTextFormat(
                    w!("Consolas"),
                    None,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    26.0,
                    w!("en-us"),
                )
                .unwrap()
        };
        unsafe {
            text_format
                .SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)
                .unwrap();
        }

        let text_layout = unsafe {
            dwrite_factory
                .CreateTextLayout(&[b' ' as u16], &text_format, 0.0, 0.0)
                .unwrap()
        };

        let mut metrics = DWRITE_HIT_TEST_METRICS::default();
        let mut _dummy: (f32, f32) = (0.0, 0.0);
        unsafe {
            text_layout
                .HitTestTextPosition(0, false, &mut _dummy.0, &mut _dummy.1, &mut metrics)
                .unwrap();
        }

        let character_spacing = (metrics.width.ceil() - metrics.width) / 2.0;
        let font_size = (metrics.width.ceil(), metrics.height);

        Self {
            font_size,
            window_size,
            d3d11_device,
            d3d11_device_context,
            d3d11_blend_state,
            d3d11_rasterizer_state,
            d3d11_depth_stencil_state,
            d3d11_input_layout,
            d3d11_vertex_shader,
            d3d11_pixel_shader,
            d3d11_vertex_buffer,
            d3d11_index_buffer,
            d3d11_constant_buffer,
            d3d11_font_atlas_texture,
            d3d11_texture_sampler_linear,
            d2d1_device,
            d2d1_device_context,
            dxgi_swap_chain,
            text_format,
            character_spacing,
            dwrite_factory,
        }
    }

    pub fn resize(&self) {
        unsafe {
            self.d3d11_device_context.OMSetRenderTargets(None, None);
            self.d2d1_device_context.SetTarget(None);
            self.dxgi_swap_chain
                .ResizeBuffers(0, 0, 0, DXGI_FORMAT_B8G8R8A8_UNORM, 0)
                .unwrap();
            let d2d1_back_buffer: IDXGISurface = self.dxgi_swap_chain.GetBuffer(0).unwrap();
            let bitmap = self
                .d2d1_device_context
                .CreateBitmapFromDxgiSurface(
                    &d2d1_back_buffer,
                    Some(&D2D1_BITMAP_PROPERTIES1 {
                        pixelFormat: D2D1_PIXEL_FORMAT {
                            format: DXGI_FORMAT_B8G8R8A8_UNORM,
                            alphaMode: D2D1_ALPHA_MODE_IGNORE,
                        },
                        bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                        ..Default::default()
                    }),
                )
                .unwrap();
            self.d2d1_device_context.SetTarget(&bitmap);
        }
    }

    pub unsafe fn draw(
        &self,
        theme: &Theme,
        buffers: &HashMap<Url, Buffer>,
        render_data: &RenderData,
    ) {
        let draw_data = render_data.draw_data;

        let viewport = D3D11_VIEWPORT {
            TopLeftX: 0.0,
            TopLeftY: 0.0,
            Width: draw_data.display_size[0],
            Height: draw_data.display_size[1],
            MinDepth: 0.0,
            MaxDepth: 1.0,
        };

        let d3d11_rtv = {
            let mut rtv = None;
            let d3d11_back_buffer: ID3D11Resource = self.dxgi_swap_chain.GetBuffer(0).unwrap();
            self.d3d11_device
                .CreateRenderTargetView(&d3d11_back_buffer, None, Some(&mut rtv))
                .unwrap();
            rtv.unwrap()
        };
        self.d3d11_device_context
            .OMSetRenderTargets(Some(&[Some(d3d11_rtv.clone())]), None);
        self.d3d11_device_context.ClearRenderTargetView(
            &d3d11_rtv,
            &theme.background_color.into_imvec(1.0) as *const ImVec4 as *const _,
        );
        self.d3d11_device_context.RSSetViewports(Some(&[viewport]));
        self.d3d11_device_context
            .IASetInputLayout(&self.d3d11_input_layout);
        self.d3d11_device_context.IASetVertexBuffers(
            0,
            1,
            Some(&Some(self.d3d11_vertex_buffer.clone())),
            Some(&(std::mem::size_of::<DrawVert>() as u32)),
            Some(&0),
        );
        self.d3d11_device_context.IASetIndexBuffer(
            &self.d3d11_index_buffer,
            DXGI_FORMAT_R16_UINT,
            0,
        );
        self.d3d11_device_context
            .IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        self.d3d11_device_context
            .VSSetShader(&self.d3d11_vertex_shader, None);
        self.d3d11_device_context
            .VSSetConstantBuffers(0, Some(&[Some(self.d3d11_constant_buffer.clone())]));
        self.d3d11_device_context
            .PSSetShader(&self.d3d11_pixel_shader, None);
        self.d3d11_device_context
            .OMSetBlendState(&self.d3d11_blend_state, Some(&0.0), u32::MAX);
        self.d3d11_device_context
            .OMSetDepthStencilState(&self.d3d11_depth_stencil_state, 0);
        self.d3d11_device_context
            .RSSetState(&self.d3d11_rasterizer_state);

        let mut constant_data = D3D11_MAPPED_SUBRESOURCE::default();
        self.d3d11_device_context
            .Map(
                &self.d3d11_constant_buffer,
                0,
                D3D11_MAP_WRITE_DISCARD,
                0,
                Some(&mut constant_data),
            )
            .unwrap();
        let mut vertex_data = D3D11_MAPPED_SUBRESOURCE::default();
        self.d3d11_device_context
            .Map(
                &self.d3d11_vertex_buffer,
                0,
                D3D11_MAP_WRITE_DISCARD,
                0,
                Some(&mut vertex_data),
            )
            .unwrap();
        let mut index_data = D3D11_MAPPED_SUBRESOURCE::default();
        self.d3d11_device_context
            .Map(
                &self.d3d11_index_buffer,
                0,
                D3D11_MAP_WRITE_DISCARD,
                0,
                Some(&mut index_data),
            )
            .unwrap();

        let mut vertex_dest = slice::from_raw_parts_mut(
            vertex_data.pData.cast::<DrawVert>(),
            draw_data.total_vtx_count as usize,
        );
        let mut index_dest = slice::from_raw_parts_mut(
            index_data.pData.cast::<DrawIdx>(),
            draw_data.total_idx_count as usize,
        );

        for (vertex_buffer, index_buffer) in draw_data
            .draw_lists()
            .map(|draw_list| (draw_list.vtx_buffer(), draw_list.idx_buffer()))
        {
            vertex_dest[..vertex_buffer.len()].copy_from_slice(vertex_buffer);
            index_dest[..index_buffer.len()].copy_from_slice(index_buffer);
            vertex_dest = &mut vertex_dest[vertex_buffer.len()..];
            index_dest = &mut index_dest[index_buffer.len()..];
        }

        self.d3d11_device_context
            .Unmap(&self.d3d11_vertex_buffer, 0);
        self.d3d11_device_context.Unmap(&self.d3d11_index_buffer, 0);

        let l = draw_data.display_pos[0];
        let r = draw_data.display_pos[0] + draw_data.display_size[0];
        let t = draw_data.display_pos[1];
        let b = draw_data.display_pos[1] + draw_data.display_size[1];
        let constants = Constants {
            projection: [
                [2.0 / (r - l), 0.0, 0.0, 0.0],
                [0.0, 2.0 / (t - b), 0.0, 0.0],
                [0.0, 0.0, 0.5, 0.0],
                [(r + l) / (l - r), (t + b) / (b - t), 0.5, 1.0],
            ],
        };
        std::ptr::copy_nonoverlapping(&constants, constant_data.pData as *mut _, 1);

        self.d3d11_device_context
            .Unmap(&self.d3d11_constant_buffer, 0);

        let clip_off = draw_data.display_pos;
        let mut vertex_offset = 0;
        let mut index_offset = 0;
        self.d3d11_device_context
            .PSSetShaderResources(0, Some(&[Some(self.d3d11_font_atlas_texture.clone())]));
        self.d3d11_device_context
            .PSSetSamplers(0, Some(&[Some(self.d3d11_texture_sampler_linear.clone())]));
        for draw_list in draw_data.draw_lists() {
            for cmd in draw_list.commands() {
                match cmd {
                    DrawCmd::Elements {
                        count,
                        cmd_params:
                            DrawCmdParams {
                                clip_rect,
                                texture_id,
                                ..
                            },
                    } => {
                        let scissor = RECT {
                            left: (clip_rect[0] - clip_off[0]) as i32,
                            top: (clip_rect[1] - clip_off[1]) as i32,
                            right: (clip_rect[2] - clip_off[0]) as i32,
                            bottom: (clip_rect[3] - clip_off[1]) as i32,
                        };
                        self.d3d11_device_context
                            .RSSetScissorRects(Some(&[scissor]));

                        if texture_id.id() != usize::MAX {
                            let url = render_data.buffers.get(texture_id.id());
                            if let Some(url) = &url {
                                let (buffer, scroll_state, clip_rect) = (
                                    buffers.get(url).unwrap(),
                                    render_data.scroll_state.get(url).unwrap(),
                                    render_data.clip_rects.get(url).unwrap(),
                                );

                                let lines_scrolled = (
                                    scroll_state.0 / self.font_size.0,
                                    scroll_state.1 / self.font_size.1,
                                );
                                let vertical_lines_visible =
                                    ((clip_rect.Max.y - clip_rect.Min.y) / self.font_size.1).ceil();
                                let text_position = (
                                    clip_rect.Min.x - (lines_scrolled.0 * self.font_size.0),
                                    clip_rect.Min.y - (lines_scrolled.1.fract() * self.font_size.1),
                                );

                                let text = buffer.piece_table.text_between_lines(
                                    lines_scrolled.1.floor() as usize,
                                    lines_scrolled.1.floor() as usize
                                        + vertical_lines_visible as usize,
                                );

                                let mut effects = vec![TextEffect {
                                    kind: TextEffectKind::ForegroundColor(theme.foreground_color),
                                    start: 0,
                                    length: text.len(),
                                }];

                                if let Some(syntect) = &buffer.syntect {
                                    effects.extend(syntect.highlight_lines(
                                        &buffer.piece_table,
                                        lines_scrolled.1.floor() as usize,
                                        lines_scrolled.1.floor() as usize
                                            + vertical_lines_visible as usize
                                            + 1,
                                    ))
                                }

                                let text_offset = buffer
                                    .piece_table
                                    .char_index_from_line_col(lines_scrolled.1.floor() as usize, 0)
                                    .unwrap_or(0);
                                if buffer.mode != BufferMode::Insert {
                                    for cursor in &buffer.cursors {
                                        if text_offset <= cursor.position {
                                            effects.push(TextEffect {
                                                kind: TextEffectKind::ForegroundColor(
                                                    theme.background_color,
                                                ),
                                                start: cursor.position - text_offset,
                                                length: 1,
                                            });
                                        }
                                    }
                                }

                                let clip_rect = D2D_RECT_F {
                                    left: scissor.left as f32,
                                    top: scissor.top as f32,
                                    right: scissor.right as f32,
                                    bottom: scissor.bottom as f32,
                                };
                                self.draw_text(&text, &effects, &clip_rect, text_position);
                            }
                        } else {
                            self.d3d11_device_context.DrawIndexed(
                                count as u32,
                                index_offset as u32,
                                vertex_offset as i32,
                            );
                        }

                        index_offset += count;
                    }
                    _ => panic!(),
                }
            }
            vertex_offset += draw_list.vtx_buffer().len();
        }

        self.dxgi_swap_chain.Present(0, 0).unwrap();
    }

    pub unsafe fn draw_text(
        &self,
        text: &[u8],
        effects: &[TextEffect],
        clip_rect: &D2D_RECT_F,
        text_position: (f32, f32),
    ) {
        self.d2d1_device_context.BeginDraw();
        self.d2d1_device_context
            .PushAxisAlignedClip(clip_rect, D2D1_ANTIALIAS_MODE_ALIASED);

        // Col offset text will not use conversion because only ASCII is allowed
        let mut wide_text = vec![];
        for c in text {
            wide_text.push(*c as u16);
        }

        let text_layout = self
            .dwrite_factory
            .CreateTextLayout(&wide_text, &self.text_format, f32::MAX, f32::MAX)
            .unwrap();

        text_layout
            .cast::<IDWriteTextLayout1>()
            .unwrap()
            .SetCharacterSpacing(
                self.character_spacing,
                self.character_spacing,
                self.character_spacing,
                DWRITE_TEXT_RANGE {
                    startPosition: 0,
                    length: wide_text.len() as u32,
                },
            )
            .unwrap();

        for effect in effects {
            match &effect.kind {
                TextEffectKind::ForegroundColor(color) => unsafe {
                    let brush = self
                        .d2d1_device_context
                        .CreateSolidColorBrush(
                            &D2D1_COLOR_F {
                                r: color.r,
                                g: color.g,
                                b: color.b,
                                a: 1.0,
                            },
                            Some(&DEFAULT_BRUSH_PROPERTIES),
                        )
                        .unwrap();

                    text_layout
                        .SetDrawingEffect(
                            &brush,
                            DWRITE_TEXT_RANGE {
                                startPosition: effect.start as u32,
                                length: effect.length as u32,
                            },
                        )
                        .unwrap();
                },
            }
        }

        let brush = self
            .d2d1_device_context
            .CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
                Some(&DEFAULT_BRUSH_PROPERTIES),
            )
            .unwrap();

        self.d2d1_device_context.DrawTextLayout(
            D2D_POINT_2F {
                x: text_position.0,
                y: text_position.1,
            },
            &text_layout,
            &brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
        );

        self.d2d1_device_context.PopAxisAlignedClip();
        self.d2d1_device_context.EndDraw(None, None).unwrap();
    }
}

impl Color {
    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            r_u8: r,
            g_u8: g,
            b_u8: b,
        }
    }
}

const DEFAULT_BRUSH_PROPERTIES: D2D1_BRUSH_PROPERTIES = D2D1_BRUSH_PROPERTIES {
    opacity: 1.0,
    transform: Matrix3x2::identity(),
};

#[derive(Default)]
#[repr(C)]
struct Constants {
    projection: [[f32; 4]; 4],
}

const PIXEL_SHADER: [u8; 736] = [
    68, 88, 66, 67, 125, 103, 79, 95, 222, 121, 148, 19, 194, 16, 131, 143, 142, 39, 156, 52, 1, 0,
    0, 0, 224, 2, 0, 0, 5, 0, 0, 0, 52, 0, 0, 0, 244, 0, 0, 0, 104, 1, 0, 0, 156, 1, 0, 0, 68, 2,
    0, 0, 82, 68, 69, 70, 184, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 60, 0, 0, 0, 0, 5, 255,
    255, 0, 1, 0, 0, 142, 0, 0, 0, 82, 68, 49, 49, 60, 0, 0, 0, 24, 0, 0, 0, 32, 0, 0, 0, 40, 0, 0,
    0, 36, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0, 0, 124, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 133, 0, 0, 0, 2, 0, 0, 0, 5, 0, 0, 0, 4, 0, 0, 0,
    255, 255, 255, 255, 0, 0, 0, 0, 1, 0, 0, 0, 12, 0, 0, 0, 115, 97, 109, 112, 108, 101, 114, 48,
    0, 116, 101, 120, 116, 117, 114, 101, 48, 0, 77, 105, 99, 114, 111, 115, 111, 102, 116, 32, 40,
    82, 41, 32, 72, 76, 83, 76, 32, 83, 104, 97, 100, 101, 114, 32, 67, 111, 109, 112, 105, 108,
    101, 114, 32, 49, 48, 46, 49, 0, 171, 171, 73, 83, 71, 78, 108, 0, 0, 0, 3, 0, 0, 0, 8, 0, 0,
    0, 80, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 15, 0, 0, 0, 92, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 1, 0, 0, 0, 15, 15, 0, 0, 98, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3,
    0, 0, 0, 2, 0, 0, 0, 3, 3, 0, 0, 83, 86, 95, 80, 79, 83, 73, 84, 73, 79, 78, 0, 67, 79, 76, 79,
    82, 0, 84, 69, 88, 67, 79, 79, 82, 68, 0, 171, 79, 83, 71, 78, 44, 0, 0, 0, 1, 0, 0, 0, 8, 0,
    0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 15, 0, 0, 0, 83, 86, 95, 84,
    97, 114, 103, 101, 116, 0, 171, 171, 83, 72, 69, 88, 160, 0, 0, 0, 80, 0, 0, 0, 40, 0, 0, 0,
    106, 8, 0, 1, 90, 0, 0, 3, 0, 96, 16, 0, 0, 0, 0, 0, 88, 24, 0, 4, 0, 112, 16, 0, 0, 0, 0, 0,
    85, 85, 0, 0, 98, 16, 0, 3, 242, 16, 16, 0, 1, 0, 0, 0, 98, 16, 0, 3, 50, 16, 16, 0, 2, 0, 0,
    0, 101, 0, 0, 3, 242, 32, 16, 0, 0, 0, 0, 0, 104, 0, 0, 2, 1, 0, 0, 0, 69, 0, 0, 139, 194, 0,
    0, 128, 67, 85, 21, 0, 242, 0, 16, 0, 0, 0, 0, 0, 70, 16, 16, 0, 2, 0, 0, 0, 70, 126, 16, 0, 0,
    0, 0, 0, 0, 96, 16, 0, 0, 0, 0, 0, 56, 0, 0, 7, 242, 32, 16, 0, 0, 0, 0, 0, 70, 14, 16, 0, 0,
    0, 0, 0, 70, 30, 16, 0, 1, 0, 0, 0, 62, 0, 0, 1, 83, 84, 65, 84, 148, 0, 0, 0, 3, 0, 0, 0, 1,
    0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

const VERTEX_SHADER: [u8; 980] = [
    68, 88, 66, 67, 48, 44, 88, 70, 235, 253, 22, 149, 231, 166, 24, 135, 120, 121, 96, 121, 1, 0,
    0, 0, 212, 3, 0, 0, 5, 0, 0, 0, 52, 0, 0, 0, 72, 1, 0, 0, 184, 1, 0, 0, 44, 2, 0, 0, 56, 3, 0,
    0, 82, 68, 69, 70, 12, 1, 0, 0, 1, 0, 0, 0, 108, 0, 0, 0, 1, 0, 0, 0, 60, 0, 0, 0, 0, 5, 254,
    255, 0, 1, 0, 0, 228, 0, 0, 0, 82, 68, 49, 49, 60, 0, 0, 0, 24, 0, 0, 0, 32, 0, 0, 0, 40, 0, 0,
    0, 36, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0, 0, 92, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 118, 101, 114, 116, 101, 120, 66, 117, 102, 102, 101,
    114, 0, 171, 171, 171, 92, 0, 0, 0, 1, 0, 0, 0, 132, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 172, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 2, 0, 0, 0, 192, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255,
    255, 0, 0, 0, 0, 255, 255, 255, 255, 0, 0, 0, 0, 112, 114, 111, 106, 101, 99, 116, 105, 111,
    110, 0, 102, 108, 111, 97, 116, 52, 120, 52, 0, 3, 0, 3, 0, 4, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 183, 0, 0, 0, 77, 105, 99, 114, 111, 115, 111,
    102, 116, 32, 40, 82, 41, 32, 72, 76, 83, 76, 32, 83, 104, 97, 100, 101, 114, 32, 67, 111, 109,
    112, 105, 108, 101, 114, 32, 49, 48, 46, 49, 0, 73, 83, 71, 78, 104, 0, 0, 0, 3, 0, 0, 0, 8, 0,
    0, 0, 80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 3, 3, 0, 0, 89, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 1, 0, 0, 0, 15, 15, 0, 0, 95, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    3, 0, 0, 0, 2, 0, 0, 0, 3, 3, 0, 0, 80, 79, 83, 73, 84, 73, 79, 78, 0, 67, 79, 76, 79, 82, 0,
    84, 69, 88, 67, 79, 79, 82, 68, 0, 79, 83, 71, 78, 108, 0, 0, 0, 3, 0, 0, 0, 8, 0, 0, 0, 80, 0,
    0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 15, 0, 0, 0, 92, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 3, 0, 0, 0, 1, 0, 0, 0, 15, 0, 0, 0, 98, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0,
    2, 0, 0, 0, 3, 12, 0, 0, 83, 86, 95, 80, 79, 83, 73, 84, 73, 79, 78, 0, 67, 79, 76, 79, 82, 0,
    84, 69, 88, 67, 79, 79, 82, 68, 0, 171, 83, 72, 69, 88, 4, 1, 0, 0, 80, 0, 1, 0, 65, 0, 0, 0,
    106, 8, 0, 1, 89, 0, 0, 4, 70, 142, 32, 0, 0, 0, 0, 0, 4, 0, 0, 0, 95, 0, 0, 3, 50, 16, 16, 0,
    0, 0, 0, 0, 95, 0, 0, 3, 242, 16, 16, 0, 1, 0, 0, 0, 95, 0, 0, 3, 50, 16, 16, 0, 2, 0, 0, 0,
    103, 0, 0, 4, 242, 32, 16, 0, 0, 0, 0, 0, 1, 0, 0, 0, 101, 0, 0, 3, 242, 32, 16, 0, 1, 0, 0, 0,
    101, 0, 0, 3, 50, 32, 16, 0, 2, 0, 0, 0, 104, 0, 0, 2, 1, 0, 0, 0, 56, 0, 0, 8, 242, 0, 16, 0,
    0, 0, 0, 0, 86, 21, 16, 0, 0, 0, 0, 0, 70, 142, 32, 0, 0, 0, 0, 0, 1, 0, 0, 0, 50, 0, 0, 10,
    242, 0, 16, 0, 0, 0, 0, 0, 70, 142, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 6, 16, 16, 0, 0, 0, 0, 0,
    70, 14, 16, 0, 0, 0, 0, 0, 0, 0, 0, 8, 242, 32, 16, 0, 0, 0, 0, 0, 70, 14, 16, 0, 0, 0, 0, 0,
    70, 142, 32, 0, 0, 0, 0, 0, 3, 0, 0, 0, 54, 0, 0, 5, 242, 32, 16, 0, 1, 0, 0, 0, 70, 30, 16, 0,
    1, 0, 0, 0, 54, 0, 0, 5, 50, 32, 16, 0, 2, 0, 0, 0, 70, 16, 16, 0, 2, 0, 0, 0, 62, 0, 0, 1, 83,
    84, 65, 84, 148, 0, 0, 0, 6, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 6, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];
