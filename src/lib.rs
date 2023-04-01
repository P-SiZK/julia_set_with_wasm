mod utils;

use std::{cell::RefCell, rc::Rc};

use wasm_bindgen::prelude::*;
use web_sys::{
    HtmlCanvasElement, WebGl2RenderingContext, WebGlProgram, WebGlShader, WheelEvent, Window,
};

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

const ZOOM_IN: f32 = 0.8;

static VERTEX_SHADER: &'static str = r#"#version 300 es
    in vec2 a_position;

    void main() {
        gl_Position = vec4(a_position, 0.0, 1.0);
    }
"#;

static FRAGMENT_SHADER: &'static str = r#"#version 300 es
    precision highp float;
    precision highp int;

    uniform vec2	min;
    uniform vec2	max;

    uniform vec2	resolution;
    uniform int		iterations;

    out vec4 fragmentColor;

    vec3 Mandelbrot(vec2 c) {
        vec2 z = c;
        for(int i = 1; i <= iterations ; ++i) {
            vec2 z2 = z * z;
            if (z2.x + z2.y > 4.0) return vec3(z, float(i));

            z = vec2(
                (z2.x - z2.y),
                (z.y * z.x * 2.0)
            ) + c;
        }
        return vec3(z, 0.);
    }

    vec4 Colors(int i) {
        int n = i % 16;
        if (n ==  0) return vec4( 66.,  30.,  15., 255.) / 255.;
        if (n ==  1) return vec4( 25.,   7.,  26., 255.) / 255.;
        if (n ==  2) return vec4(  9.,   1.,  47., 255.) / 255.;
        if (n ==  3) return vec4(  4.,   4.,  73., 255.) / 255.;
        if (n ==  4) return vec4(  0.,   7., 100., 255.) / 255.;
        if (n ==  5) return vec4( 12.,  44., 138., 255.) / 255.;
        if (n ==  6) return vec4( 24.,  82., 177., 255.) / 255.;
        if (n ==  7) return vec4( 57., 125., 209., 255.) / 255.;
        if (n ==  8) return vec4(134., 181., 229., 255.) / 255.;
        if (n ==  9) return vec4(211., 236., 248., 255.) / 255.;
        if (n == 10) return vec4(241., 233., 191., 255.) / 255.;
        if (n == 11) return vec4(248., 201.,  95., 255.) / 255.;
        if (n == 12) return vec4(255., 170.,   0., 255.) / 255.;
        if (n == 13) return vec4(204., 128.,   0., 255.) / 255.;
        if (n == 14) return vec4(153.,  87.,   0., 255.) / 255.;
        if (n == 15) return vec4(106,   52.,   3., 255.) / 255.;
    }

    // https://en.wikipedia.org/wiki/Plotting_algorithms_for_the_Mandelbrot_set
    vec4 Color(int i, vec2 z) {
        float log_zn = log(z.x * z.x + z.y * z.y) / 2.;
        float nu = log2(log_zn / log(2.));
        float it = float(i) + 1. - nu;

        i = int(floor(it));
        vec4 color1 = Colors(i);
        vec4 color2 = Colors(i + 1);
        return mix(color1, color2, fract(it));
    }

    void main() {
        vec3 m = Mandelbrot(
            vec2(
                min.x + (max.x - min.x) * gl_FragCoord.x / resolution.x,
            	min.y + (max.y - min.y) * gl_FragCoord.y / resolution.y
            )
        );
        vec2 z = vec2(m.x, m.y);
        int i = int(m.z);
        fragmentColor = i == 0 ?
            vec4(0.0, 0.0, 0.0, 1.0) :
            Color(i, z);
    }
"#;

const VERTICES: [f32; 12] = [
    -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0,
];

#[wasm_bindgen]
pub async fn start() -> Result<(), JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window exists"))?;
    let document = window
        .document()
        .ok_or_else(|| JsValue::from_str("should have document"))?;
    let body = document
        .body()
        .ok_or_else(|| JsValue::from_str("no body exists"))?;
    let width = window
        .inner_width()?
        .as_f64()
        .ok_or_else(|| JsValue::from_str("fail to convert inner width"))? as u32;
    let height = window
        .inner_height()?
        .as_f64()
        .ok_or_else(|| JsValue::from_str("fail to convert inner height"))? as u32;

    let canvas = document
        .create_element("canvas")?
        .dyn_into::<HtmlCanvasElement>()?;
    canvas.set_width(width);
    canvas.set_height(height);
    body.append_child(&canvas)?;

    let context = canvas
        .get_context("webgl2")?
        .ok_or_else(|| JsValue::from_str("fail to get context"))?
        .dyn_into::<WebGl2RenderingContext>()?;

    let program = link_program(&context)?;

    let uniform_min = context
        .get_uniform_location(&program, "min")
        .ok_or_else(|| JsValue::from_str("fali to get uniform location"))?;
    let uniform_max = context
        .get_uniform_location(&program, "max")
        .ok_or_else(|| JsValue::from_str("fali to get uniform location"))?;
    let uniform_resolution = context
        .get_uniform_location(&program, "resolution")
        .ok_or_else(|| JsValue::from_str("fali to get uniform location"))?;
    let uniform_iterations = context
        .get_uniform_location(&program, "iterations")
        .ok_or_else(|| JsValue::from_str("fali to get uniform location"))?;

    let iterations = 100;
    let zoom = 1.8;
    let re_center = -0.7;
    let im_center = 0.;
    let ratio = width as f32 / height as f32;
    let re_min = re_center - zoom;
    let re_max = re_center + zoom;
    let im_min = im_center - zoom / ratio;
    let im_max = im_center + zoom / ratio;

    context.uniform2f(Some(&uniform_min), re_min, im_min);
    context.uniform2f(Some(&uniform_max), re_max, im_max);
    context.uniform2f(Some(&uniform_resolution), width as f32, height as f32);
    context.uniform1i(Some(&uniform_iterations), iterations);

    draw(&context, &program)?;

    let iterations = Rc::new(RefCell::new(iterations));
    let zoom = Rc::new(RefCell::new(zoom));
    let center = Rc::new(RefCell::new(vec![re_center, im_center]));
    let min = Rc::new(RefCell::new(vec![re_min, im_min]));
    let max = Rc::new(RefCell::new(vec![re_max, im_max]));

    on_resize(&window, &program, &context, &zoom, &center, &min, &max)?;

    on_wheel(
        &window,
        &program,
        &context,
        &iterations,
        &zoom,
        &center,
        &min,
        &max,
    )?;

    Ok(())
}

fn on_resize(
    window: &Window,
    program: &WebGlProgram,
    context: &WebGl2RenderingContext,
    zoom: &Rc<RefCell<f32>>,
    center: &Rc<RefCell<Vec<f32>>>,
    min: &Rc<RefCell<Vec<f32>>>,
    max: &Rc<RefCell<Vec<f32>>>,
) -> Result<(), JsValue> {
    let new_window = window.clone();
    let context = context.clone();
    let program = program.clone();
    let canvas = context
        .canvas()
        .unwrap_throw()
        .dyn_into::<HtmlCanvasElement>()?;
    let uniform_min = context
        .get_uniform_location(&program, "min")
        .ok_or_else(|| JsValue::from_str("fali to get uniform location"))?;
    let uniform_max = context
        .get_uniform_location(&program, "max")
        .ok_or_else(|| JsValue::from_str("fali to get uniform location"))?;
    let uniform_resolution = context
        .get_uniform_location(&program, "resolution")
        .ok_or_else(|| JsValue::from_str("fali to get uniform location"))?;
    let zoom = zoom.clone();
    let center = center.clone();
    let min = min.clone();
    let max = max.clone();
    let closure = Closure::<dyn FnMut()>::new(move || {
        let width = new_window
            .inner_width()
            .unwrap_throw()
            .as_f64()
            .ok_or_else(|| JsValue::from_str("fail to convert inner width"))
            .unwrap_throw() as u32;
        let height = new_window
            .inner_height()
            .unwrap_throw()
            .as_f64()
            .ok_or_else(|| JsValue::from_str("fail to convert inner height"))
            .unwrap_throw() as u32;

        canvas.set_width(width);
        canvas.set_height(height);

        let ratio = width as f32 / height as f32;

        let zoom = zoom.borrow();
        let center = center.borrow();
        let re_center = center.first().unwrap_throw();
        let im_center = center.last().unwrap_throw();
        let re_min = re_center - *zoom;
        let re_max = re_center + *zoom;
        let im_min = im_center - *zoom / ratio;
        let im_max = im_center + *zoom / ratio;
        *min.borrow_mut() = vec![re_min, im_min];
        *max.borrow_mut() = vec![re_max, im_max];

        context.uniform2f(Some(&uniform_min), re_min, im_min);
        context.uniform2f(Some(&uniform_max), re_max, im_max);
        context.uniform2f(Some(&uniform_resolution), width as f32, height as f32);

        context.viewport(0, 0, width as i32, height as i32);

        draw(&context, &program).unwrap_throw();
    });
    window.set_onresize(Some(closure.as_ref().unchecked_ref()));
    closure.forget();

    Ok(())
}

fn on_wheel(
    window: &Window,
    program: &WebGlProgram,
    context: &WebGl2RenderingContext,
    iterations: &Rc<RefCell<i32>>,
    zoom: &Rc<RefCell<f32>>,
    center: &Rc<RefCell<Vec<f32>>>,
    min: &Rc<RefCell<Vec<f32>>>,
    max: &Rc<RefCell<Vec<f32>>>,
) -> Result<(), JsValue> {
    let new_window = window.clone();
    let context = context.clone();
    let program = program.clone();
    let uniform_min = context
        .get_uniform_location(&program, "min")
        .ok_or_else(|| JsValue::from_str("fali to get uniform location"))?;
    let uniform_max = context
        .get_uniform_location(&program, "max")
        .ok_or_else(|| JsValue::from_str("fali to get uniform location"))?;
    let uniform_iterations = context
        .get_uniform_location(&program, "iterations")
        .ok_or_else(|| JsValue::from_str("fali to get uniform location"))?;
    let iterations = iterations.clone();
    let zoom = zoom.clone();
    let center = center.clone();
    let min = min.clone();
    let max = max.clone();
    let closure = Closure::<dyn FnMut(_)>::new(move |event: WheelEvent| {
        let width = new_window
            .inner_width()
            .unwrap_throw()
            .as_f64()
            .ok_or_else(|| JsValue::from_str("fail to convert inner width"))
            .unwrap_throw() as f32;
        let height = new_window
            .inner_height()
            .unwrap_throw()
            .as_f64()
            .ok_or_else(|| JsValue::from_str("fail to convert inner height"))
            .unwrap_throw() as f32;

        let zoom_flag = event.delta_y() < 0.;
        let ratio = width / height;

        let mut iterations = iterations.borrow_mut();
        let mut zoom = zoom.borrow_mut();
        let mut center = center.borrow_mut();
        let mut re_center = *center.first().unwrap_throw();
        let mut im_center = *center.last().unwrap_throw();
        if zoom_flag {
            *iterations = (*iterations as f32 * 1.1).round() as i32;
            re_center +=
                (event.client_x() as f32 - (width / 2.)) / (width / 2.) * (*zoom * (1. - ZOOM_IN));
            im_center -= (event.client_y() as f32 - (height / 2.)) / (height / 2.)
                * (*zoom * (1. - ZOOM_IN));
        }
        *zoom *= if zoom_flag { ZOOM_IN } else { 1. / ZOOM_IN };
        if !zoom_flag {
            *iterations = (*iterations as f32 / 1.1).round() as i32;
            re_center -=
                (event.client_x() as f32 - (width / 2.)) / (width / 2.) * (*zoom * (1. - ZOOM_IN));
            im_center += (event.client_y() as f32 - (height / 2.)) / (height / 2.)
                * (*zoom * (1. - ZOOM_IN));
        }
        let re_min = re_center - *zoom;
        let re_max = re_center + *zoom;
        let im_min = im_center - *zoom / ratio;
        let im_max = im_center + *zoom / ratio;
        *min.borrow_mut() = vec![re_min, im_min];
        *max.borrow_mut() = vec![re_max, im_max];
        *center = vec![re_center, im_center];

        context.uniform2f(Some(&uniform_min), re_min, im_min);
        context.uniform2f(Some(&uniform_max), re_max, im_max);
        context.uniform1i(Some(&uniform_iterations), *iterations);

        draw(&context, &program).unwrap_throw();
    });
    window.set_onwheel(Some(closure.as_ref().unchecked_ref()));
    closure.forget();

    Ok(())
}

fn draw(context: &WebGl2RenderingContext, program: &WebGlProgram) -> Result<(), JsValue> {
    // context.clear_color(0.0, 0.0, 0.0, 1.0);
    // context.clear(WebGl2RenderingContext::COLOR_BUFFER_BIT);

    let attribute_position = context.get_attrib_location(program, "a_position");
    let buffer = context
        .create_buffer()
        .ok_or_else(|| JsValue::from_str("fail to create buffer"))?;
    context.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&buffer));
    unsafe {
        context.buffer_data_with_array_buffer_view(
            WebGl2RenderingContext::ARRAY_BUFFER,
            &js_sys::Float32Array::view(&VERTICES),
            WebGl2RenderingContext::STATIC_DRAW,
        );
    }
    context.vertex_attrib_pointer_with_f64(
        attribute_position as u32,
        2,
        WebGl2RenderingContext::FLOAT,
        false,
        0,
        0.,
    );
    context.enable_vertex_attrib_array(attribute_position as u32);
    context.draw_arrays(
        WebGl2RenderingContext::TRIANGLES,
        0,
        (VERTICES.len() / 2) as i32,
    );
    context.disable_vertex_attrib_array(attribute_position as u32);

    Ok(())
}

pub fn compile_shader(
    context: &WebGl2RenderingContext,
    shader_type: u32,
    source: &str,
) -> Result<WebGlShader, String> {
    let shader = context
        .create_shader(shader_type)
        .ok_or_else(|| String::from("unable to create shader object"))?;
    context.shader_source(&shader, source);
    context.compile_shader(&shader);

    if context
        .get_shader_parameter(&shader, WebGl2RenderingContext::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(shader)
    } else {
        Err(context
            .get_shader_info_log(&shader)
            .unwrap_or_else(|| String::from("unknown error creating shader")))
    }
}

pub fn link_program(context: &WebGl2RenderingContext) -> Result<WebGlProgram, String> {
    let vert_shader = compile_shader(
        &context,
        WebGl2RenderingContext::VERTEX_SHADER,
        VERTEX_SHADER,
    )?;
    let frag_shader = compile_shader(
        &context,
        WebGl2RenderingContext::FRAGMENT_SHADER,
        FRAGMENT_SHADER,
    )?;
    let program = context
        .create_program()
        .ok_or_else(|| String::from("unable to create shader object"))?;

    context.attach_shader(&program, &vert_shader);
    context.attach_shader(&program, &frag_shader);
    context.link_program(&program);

    if context
        .get_program_parameter(&program, WebGl2RenderingContext::LINK_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        context.use_program(Some(&program));
        Ok(program)
    } else {
        Err(context
            .get_program_info_log(&program)
            .unwrap_or_else(|| String::from("unknown error creating program object")))
    }
}
