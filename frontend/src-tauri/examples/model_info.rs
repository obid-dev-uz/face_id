//! ONNX modelning kirish/chiqishlarini va tract'da optimallashishini tekshiradi.
//! `cargo run --release --example model_info -- resources/models/<fayl>.onnx [H W]`
use tract_onnx::prelude::*;

fn main() -> TractResult<()> {
    let args: Vec<String> = std::env::args().collect();
    let path = &args[1];
    let model = tract_onnx::onnx()
        .with_ignore_value_info(true)
        .with_ignore_output_shapes(true)
        .model_for_path(path)?;
    for (i, o) in model.input_outlets()?.iter().enumerate() {
        println!("input {i}: {} {:?}", model.node(o.node).name, model.outlet_fact(*o)?);
    }
    for (i, o) in model.output_outlets()?.iter().enumerate() {
        println!("output {i}: {} label={:?}", model.node(o.node).name, model.outlet_label(*o));
    }
    let (h, w) = match (args.get(2), args.get(3)) {
        (Some(h), Some(w)) => (h.parse()?, w.parse()?),
        _ => (112usize, 112usize),
    };
    let t0 = std::time::Instant::now();
    let plan = model
        .with_input_fact(0, f32::fact([1, 3, h, w]).into())?
        .into_optimized()?
        .into_runnable()?;
    println!("optimize: {:?}", t0.elapsed());
    let input: Tensor = tract_ndarray::Array4::<f32>::zeros((1, 3, h, w)).into();
    let mut out = plan.run(tvec!(input.clone().into()))?;
    let t0 = std::time::Instant::now();
    for _ in 0..10 {
        out = plan.run(tvec!(input.clone().into()))?;
    }
    println!("run (o'rtacha 10): {:?}", t0.elapsed() / 10);
    for (i, o) in out.iter().enumerate() {
        println!("out {i}: {:?}", o.shape());
    }
    Ok(())
}
