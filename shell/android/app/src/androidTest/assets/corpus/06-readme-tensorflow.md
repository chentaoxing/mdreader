# TensorFlow

<div align="center">
  <img src="https://www.tensorflow.org/images/tf_logo_social.png" alt="TensorFlow logo"><br><br>
</div>

[![Python](https://img.shields.io/pypi/pyversions/tensorflow.svg?style=plastic)](https://badge.fury.io/py/tensorflow)
[![Documentation](https://img.shields.io/badge/api-reference-blue.svg)](https://www.tensorflow.org/api_docs)

**`Documentation`** |
------------------- |
[📘 User-facing Documentation](https://www.tensorflow.org/guide) |
[📗 Developer Documentation](https://www.tensorflow.org/community/contribute) |

## About TensorFlow

**TensorFlow** is an end-to-end open source platform for machine learning. It has a comprehensive, flexible ecosystem of [tools](https://www.tensorflow.org/resources/tools), [libraries](https://www.tensorflow.org/resources/libraries-extensions), and [community](https://www.tensorflow.org/community) resources that lets researchers push the state-of-the-art in ML and developers easily build and deploy ML-powered applications.

### Key Features

- **Flexible ecosystem of tools, libraries and community resources**
- **Robust ML production anywhere**
- **Powerful experimentation for research**
- **Accelerate your ML workflow**

## Installation

### Using pip

```bash
# Install TensorFlow
pip install tensorflow

# For GPU support
pip install tensorflow-gpu
```

### Verify installation

```python
import tensorflow as tf
print(tf.__version__)
print("Num GPUs Available: ", len(tf.config.list_physical_devices('GPU')))
```

## Quick Start

### Basic Example

```python
import tensorflow as tf

# Load and prepare the MNIST dataset
mnist = tf.keras.datasets.mnist
(x_train, y_train), (x_test, y_test) = mnist.load_data()
x_train, x_test = x_train / 255.0, x_test / 255.0

# Build the model
model = tf.keras.models.Sequential([
  tf.keras.layers.Flatten(input_shape=(28, 28)),
  tf.keras.layers.Dense(128, activation='relu'),
  tf.keras.layers.Dropout(0.2),
  tf.keras.layers.Dense(10)
])

# Compile and train
model.compile(optimizer='adam',
              loss=tf.keras.losses.SparseCategoricalCrossentropy(from_logits=True),
              metrics=['accuracy'])

model.fit(x_train, y_train, epochs=5)
model.evaluate(x_test,  y_test, verbose=2)
```

## Resources

- [TensorFlow Tutorials](https://www.tensorflow.org/tutorials)
- [TensorFlow Blog](https://blog.tensorflow.org/)
- [TensorFlow YouTube Channel](https://www.youtube.com/tensorflow)

## Community

- [TensorFlow Forum](https://discuss.tensorflow.org/)
- [Stack Overflow](https://stackoverflow.com/questions/tagged/tensorflow)
- [GitHub Discussions](https://github.com/tensorflow/tensorflow/discussions)

## License

[Apache License 2.0](https://github.com/tensorflow/tensorflow/blob/master/LICENSE)
