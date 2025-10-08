/*use onnxruntime::environment::Environment;
use onnxruntime::GraphOptimizationLevel;
use onnxruntime::tensor::OrtOwnedTensor;

fn load_model() -> Result<(), Box<Box<dyn std::error::Error>>> {
    let environment = Environment::builder()
        .with_name("openWakeWord")
        .build()?;

    // Load the ONNX model
    let session = environment
        .new_session_builder()?
        .with_graph_optimization_level(GraphOptimizationLevel::All)?
        .with_model_from_file("openwakeword_model.onnx")?;

    // Prepare your audio input (e.g., a vector of floats)
    let audio_input: Vec<f32> = vec![/* your audio data */];

    // Create an input tensor
    let input_tensor = vec![audio_input];

    // Run inference
    let outputs: Vec<OrtOwnedTensor<f32, _>> = session.run(input_tensor)?;

    // Process the output
    for output in outputs {
        println!("Output: {:?}", output);
    }

    Ok(())
}*/