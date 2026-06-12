"""
TRIBE Extensions - NVIDIA DLI Upgrades

Modules d'amélioration basés sur les formations NVIDIA DLI.

Modules disponibles:
- tribe_tensorrt: Optimisation TensorRT (+10-50x vitesse)
- tribe_rapids: Data Science GPU RAPIDS (+50-100x données)
- tribe_nemo: NVIDIA NeMo Integration (LLM management)
- tribe_finetune: LLM Fine-tuning (LoRA/QLoRA)
- tribe_prompts: Prompt Engineering (+5-15% qualité)
- tribe_advanced_rag: Advanced RAG (+15-25% précision)
- tribe_cuda: CUDA Optimization (+10-100x custom)
- tribe_multimodal: Multi-Modal (images, audio)

Usage:
    from tribe_extensions import (
        TensorRTOptimizer,
        RAPIDSDataProcessor,
        NeMoLLMManager,
        LLMFineTuner,
        PromptLibrary,
        AdvancedRAG,
        CUDAKernels,
        MultiModalEmbeddings
    )
"""

# Imports
from .tribe_tensorrt import TensorRTOptimizer, TensorRTEmbeddingWrapper
from .tribe_rapids import RAPIDSDataProcessor, RAPIDSFeatureStore
from .tribe_nemo import NeMoLLMManager, NeMoSpeechProcessor, NeMoRAGPipeline
from .tribe_finetune import LLMFineTuner, QLoRAFineTuner
from .tribe_prompts import PromptTemplate, PromptLibrary, PromptOptimizer
from .tribe_advanced_rag import AdvancedRAG, RAGDocument, RAGResult
from .tribe_cuda import CUDAKernels, triton_cosine_similarity
from .tribe_multimodal import MultiModalEmbeddings, CLIPProcessor, WhisperProcessor

__all__ = [
    'TensorRTOptimizer',
    'TensorRTEmbeddingWrapper',
    'RAPIDSDataProcessor',
    'RAPIDSFeatureStore',
    'NeMoLLMManager',
    'NeMoSpeechProcessor',
    'NeMoRAGPipeline',
    'LLMFineTuner',
    'QLoRAFineTuner',
    'PromptTemplate',
    'PromptLibrary',
    'PromptOptimizer',
    'AdvancedRAG',
    'RAGDocument',
    'RAGResult',
    'CUDAKernels',
    'triton_cosine_similarity',
    'MultiModalEmbeddings',
    'CLIPProcessor',
    'WhisperProcessor',
]

__version__ = '1.0.0'
__author__ = 'TRIBE Team'
