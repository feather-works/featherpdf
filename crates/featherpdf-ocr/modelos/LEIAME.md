# Modelos do OCR

Modelos do PaddleOCR (PP-OCR) convertidos para ONNX, baixados de
<https://huggingface.co/monkt/paddleocr-onnx>:

- `det.onnx` — detecção de linhas, PP-OCRv3 mobile (2,4 MB);
- `rec.onnx` — reconhecimento latino, PP-OCRv5 mobile (7,9 MB);
- `dict.txt` — o alfabeto do `rec.onnx`, uma letra por linha.

Licença: Apache-2.0 (PaddlePaddle/PaddleOCR). Entram no binário por `Ocr::embutido()`.
