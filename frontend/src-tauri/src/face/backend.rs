//! Inference dvigateli abstraksiyasi. Standart: tract (sof Rust, barcha platformalarda
//! bir xil natija). `ort` keyinchalik `ort-backend` feature orqali qo'shiladi.

use std::io::Cursor;

use tract_onnx::prelude::*;

pub type Array4 = tract_ndarray::Array4<f32>;
pub type ArrayD = tract_ndarray::ArrayD<f32>;

/// tract matritsa ko'paytirishni shuncha oqimga bo'ladi (1 = bitta oqim).
pub fn executor(threads: usize) -> tract_linalg::multithread::Executor {
    use tract_linalg::multithread::Executor;
    if threads <= 1 {
        Executor::SingleThread
    } else {
        Executor::multithread_with_name(threads, "khoface-tract")
    }
}

pub use tract_linalg::multithread::multithread_tract_scope;

/// Barcha keyingi inference chaqiruvlari uchun standart oqimlar soni.
pub fn set_threads(threads: usize) {
    // tract standart holatda faqat 64+ panelli matritsalarni bo'ladi; bizning modellardagi
    // konvolyutsiyalar kichikroq, shuning uchun chegara pasaytiriladi.
    tract_linalg::multithread::set_threading_panel_threshold(8);
    tract_linalg::multithread::set_default_executor(executor(threads));
}

/// Qurilmadagi yadrolar soni, lekin 4 tadan ko'p emas: kichik yadroli telefonlarda
/// ko'proq oqim tezlik bermaydi, faqat qizdiradi.
pub fn default_threads() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(4)
}

pub trait InferenceBackend: Send + Sync {
    /// Bitta NCHW kirish, chiqishlar modeldagi tartibda.
    fn run(&self, input: Array4) -> anyhow::Result<Vec<ArrayD>>;
    fn output_names(&self) -> &[String];
}

type Plan = std::sync::Arc<TypedRunnableModel>;

pub struct TractBackend {
    plan: Plan,
    outputs: Vec<String>,
}

impl TractBackend {
    /// Model baytlardan yuklanadi: Android'da resurslar APK ichida, oddiy fayl yo'li yo'q.
    pub fn load(bytes: &[u8], input_shape: [usize; 4]) -> anyhow::Result<Self> {
        // Modelda saqlangan oraliq shakllar (masalan YuNet'da 640×640) boshqa kirish
        // o'lchamiga xalaqit beradi, shuning uchun ular qayta hisoblanadi.
        let model = tract_onnx::onnx()
            .with_ignore_value_info(true)
            .with_ignore_output_shapes(true)
            .model_for_read(&mut Cursor::new(bytes))?
            .with_input_fact(0, f32::fact(input_shape).into())?;
        // ONNX chiqish nomi (`cls_8` ...) outlet yorlig'ida, tugun nomida emas.
        let outputs = model
            .output_outlets()?
            .iter()
            .map(|o| model.outlet_label(*o).map(String::from).unwrap_or_else(|| model.node(o.node).name.clone()))
            .collect();
        let plan = model.into_optimized()?.into_runnable()?;
        Ok(Self { plan, outputs })
    }
}

impl InferenceBackend for TractBackend {
    fn run(&self, input: Array4) -> anyhow::Result<Vec<ArrayD>> {
        let out = self.plan.run(tvec!(Tensor::from(input).into()))?;
        out.into_iter()
            .map(|t| t.into_tensor().into_plain_array::<f32>())
            .collect()
    }

    fn output_names(&self) -> &[String] {
        &self.outputs
    }
}
