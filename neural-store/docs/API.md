# Neural Store API Documentation

Neural Store provides both a native Rust API and a C-compatible FFI for integration into other languages.

## Rust API

The main entry point is the `NeuralStore` struct.

### `NeuralStore`

#### `open<P: AsRef<Path>>(path: P) -> Result<Self>`
Opens or creates a new store at the specified directory.

#### `put(&mut self, id: usize, vector: Vector) -> Result<()>`
Inserts a vector into the store.
- `id`: A unique identifier for the vector.
- `vector`: The `Vector` object containing the `f32` data.

#### `get(&self, id: &usize) -> Option<Arc<Vector>>`
Retrieves a vector by its ID.

#### `search(&self, query: &[f32], k: usize) -> Vec<(usize, f32)>`
Performs a similarity search.
- `query`: The query vector as a slice.
- `k`: Number of nearest neighbors to return.
- Returns: A list of `(id, score)` pairs, sorted by similarity.

---

## C FFI (Foreign Function Interface)

Located in `src/ffi/bindings.rs`.

### Types

```c
typedef struct {
    size_t id;
    float score;
} SearchResult;
```

### Functions

#### `int ns_init()`
Initializes the global store. Returns `0` on success.

#### `int ns_put(size_t id, const float* vector, size_t len)`
Inserts a vector.
- `id`: Vector identifier.
- `vector`: Pointer to the float array.
- `len`: Length of the array.

#### `SearchResult* ns_search(const float* query, size_t len, size_t k, size_t* out_count)`
Searches for the top K similar vectors.
- Returns a pointer to an array of `SearchResult`.
- `out_count` is populated with the number of results.

#### `void ns_free(SearchResult* ptr, size_t len)`
Frees the memory allocated by `ns_search`.

---

## Core Types

### `Vector`
A wrapper around `Vec<f32>`.
- `Vector::new(Vec<f32>)`
- `Vector.len()`
- `Vector.as_slice()`

### `Metric`
Enum representing supported distance metrics:
- `Metric::L2`
- `Metric::Cosine`
- `Metric::InnerProduct`
