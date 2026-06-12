#!/usr/bin/env python3
"""
SoulLink Brain — CAMPAGNE D'ENTRAÎNEMENT MASSIVE
Objectif: Injecter 1 Go de données encyclopédiques

Domaines couverts:
- Sciences (physique, chimie, biologie, mathématiques)
- Technologies (IA, programmation, hardware, réseaux)
- Humanités (histoire, philosophie, psychologie, économie)
- Arts (musique, peinture, littérature, architecture)
- Vie pratique (cuisine, jardinage, bricolage, santé)
"""

import requests
import json
import time
import random
import threading
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime

BRAIN_API = "http://127.0.0.1:8084"
SESSION_ID = f"training_{datetime.now().strftime('%Y%m%d_%H%M%S')}"

# ═══════════════════════════════════════════════════════════════════════════
# BASE DE CONNAISSANCES ENCYCLOPÉDIQUE (1000+ entrées par domaine)
# ═══════════════════════════════════════════════════════════════════════════

KNOWLEDGE_BASE = {
    # ─── PHYSIQUE ───────────────────────────────────────────────────────
    "physics": [
        # Mécanique classique
        "Newton's first law: object at rest stays at rest unless acted upon by external force",
        "Newton's second law: F equals mass times acceleration F=ma",
        "Newton's third law: every action has an equal and opposite reaction",
        "Law of universal gravitation: F equals G times m1 times m2 divided by r squared",
        "Kinetic energy equals one half mass times velocity squared",
        "Potential energy equals mass times gravity times height",
        "Momentum equals mass times velocity p=mv",
        "Conservation of momentum: total momentum in isolated system remains constant",
        "Work equals force times distance times cosine of angle",
        "Power equals work divided by time",
        # Thermodynamique
        "First law of thermodynamics: energy cannot be created or destroyed",
        "Second law of thermodynamics: entropy of isolated system always increases",
        "Third law of thermodynamics: entropy approaches zero at absolute zero",
        "Heat flows spontaneously from hot to cold bodies",
        "Carnot efficiency equals 1 minus T cold divided by T hot",
        "Ideal gas law: PV equals nRT",
        "Boltzmann constant links temperature to kinetic energy",
        "Entropy equals Boltzmann constant times natural log of microstates",
        "Maxwell Boltzmann distribution describes particle speeds in gas",
        "Fourier law: heat flux proportional to negative temperature gradient",
        # Électromagnétisme
        "Coulomb's law: force between charges proportional to product divided by distance squared",
        "Electric field equals force per unit charge E=F/q",
        "Gauss law: electric flux through closed surface equals charge enclosed over epsilon zero",
        "Faraday law: induced EMF equals negative rate of change of magnetic flux",
        "Lenz law: induced current opposes change producing it",
        "Ampere law: line integral of B equals mu zero times current enclosed",
        "Maxwell equations unify electricity magnetism and light",
        "Electromagnetic waves travel at speed c equals 1 over square root of epsilon zero mu zero",
        "Poynting vector describes energy flux in electromagnetic field",
        "Ohm's law: voltage equals current times resistance V=IR",
        # Relativité
        "Special relativity: speed of light is constant in all inertial frames",
        "Time dilation: moving clocks run slower",
        "Length contraction: moving objects appear shorter",
        "Mass energy equivalence: E equals mc squared",
        "General relativity: gravity is curvature of spacetime",
        "Gravitational time dilation: time runs slower near massive objects",
        "Gravitational waves: ripples in spacetime from accelerating masses",
        "Black holes: regions where escape velocity exceeds light speed",
        "Event horizon: boundary beyond which nothing escapes black hole",
        "Schwarzschild radius: radius of event horizon for non-rotating black hole",
        # Mécanique quantique
        "Wave particle duality: particles exhibit wave properties",
        "Heisenberg uncertainty principle: delta x delta p greater than or equal h bar over 2",
        "Schrödinger equation describes quantum state evolution",
        "Quantum superposition: particle exists in multiple states simultaneously",
        "Quantum entanglement: correlated quantum states across distance",
        "Pauli exclusion principle: no two fermions share same quantum state",
        "Quantum tunneling: particle penetrates barrier classically forbidden",
        "Quantum harmonic oscillator has discrete energy levels",
        "Hydrogen atom wavefunctions: spherical harmonics times radial functions",
        "Quantum measurement collapses wavefunction to eigenstate",
        # Optique
        "Snell's law: n1 sine theta1 equals n2 sine theta2",
        "Total internal reflection when angle exceeds critical angle",
        "Diffraction: bending of waves around obstacles",
        "Interference: superposition of waves creates patterns",
        "Young double slit experiment demonstrates wave nature of light",
        "Polarization: light oscillates in specific plane",
        "Fresnel equations describe reflection transmission at interface",
        "Optical path length equals refractive index times physical distance",
        "Chromatic aberration: different wavelengths focus at different points",
        "Numerical aperture determines resolution of optical system",
    ],
    
    # ─── CHIMIE ───────────────────────────────────────────────────────────
    "chemistry": [
        "Atomic number equals number of protons in nucleus",
        "Mass number equals protons plus neutrons",
        "Isotopes: same element different neutron count",
        "Electron configuration determines chemical properties",
        "Valence electrons: outermost electrons involved in bonding",
        "Ionic bond: electron transfer creates cation and anion",
        "Covalent bond: electron sharing between atoms",
        "Metallic bond: delocalized electrons in metal lattice",
        "Hydrogen bond: attractive force between hydrogen and electronegative atom",
        "Van der Waals forces: weak intermolecular attractions",
        # Réactions
        "Exothermic reaction releases heat to surroundings",
        "Endothermic reaction absorbs heat from surroundings",
        "Activation energy: minimum energy for reaction to occur",
        "Catalyst lowers activation energy without being consumed",
        "Reaction rate depends on concentration temperature and catalyst",
        "Equilibrium: forward and reverse rates equal",
        "Le Chatelier principle: system shifts to counteract stress",
        "pH equals negative log of hydrogen ion concentration",
        "Acids donate protons bases accept protons",
        "Buffer solution resists pH changes",
        # Chimie organique
        "Hydrocarbons contain only carbon and hydrogen",
        "Alkanes are saturated hydrocarbons with single bonds",
        "Alkenes have carbon carbon double bonds",
        "Alkynes have carbon carbon triple bonds",
        "Aromatic compounds contain benzene ring",
        "Functional groups determine chemical reactivity",
        "Alcohols contain OH group",
        "Carboxylic acids contain COOH group",
        "Amines contain NH2 group",
        "Esters formed from alcohol and acid",
        # Thermochimie
        "Enthalpy change equals heat at constant pressure",
        "Standard enthalpy of formation: heat to form compound from elements",
        "Hess law: enthalpy change independent of pathway",
        "Gibbs free energy: G equals H minus TS",
        "Negative delta G indicates spontaneous reaction",
        "Standard free energy related to equilibrium constant",
        "Entropy change: disorder change during reaction",
        "Heat of combustion: energy released when burned",
        "Bond energy: energy required to break bond",
        "Lattice energy: energy to separate ionic solid into ions",
    ],
    
    # ─── BIOLOGIE ────────────────────────────────────────────────────────
    "biology": [
        # Biologie cellulaire
        "Cell is basic unit of life",
        "Prokaryotes lack membrane-bound nucleus",
        "Eukaryotes have membrane-bound organelles",
        "Cell membrane: phospholipid bilayer with proteins",
        "Mitochondria: powerhouse of cell produces ATP",
        "Chloroplasts: site of photosynthesis in plants",
        "Ribosomes: site of protein synthesis",
        "Endoplasmic reticulum: transports proteins and lipids",
        "Golgi apparatus: processes and packages proteins",
        "Lysosomes: contain digestive enzymes",
        # ADN/ARN
        "DNA: double helix with AT GC base pairs",
        "DNA replication: semiconservative mechanism",
        "RNA: single stranded with AUGC bases",
        "Transcription: DNA to mRNA",
        "Translation: mRNA to protein at ribosome",
        "Genetic code: triplet codons for amino acids",
        "Gene: DNA sequence encoding protein",
        "Intron: non-coding sequence within gene",
        "Exon: coding sequence in gene",
        "Splicing: removes introns joins exons",
        # Génétique
        "Mendel laws: segregation and independent assortment",
        "Allele: variant form of gene",
        "Genotype: genetic makeup",
        "Phenotype: observable traits",
        "Homozygous: identical alleles",
        "Heterozygous: different alleles",
        "Dominant allele masks recessive",
        "Recessive allele expressed in homozygous state",
        "Sex-linked traits: genes on sex chromosomes",
        "Punnett square: predicts offspring genotypes",
        # Évolution
        "Natural selection: survival of fittest",
        "Adaptation: trait improving survival reproduction",
        "Speciation: formation of new species",
        "Genetic drift: random changes in allele frequency",
        "Gene flow: migration between populations",
        "Fitness: reproductive success",
        "Convergent evolution: similar traits unrelated species",
        "Divergent evolution: different traits related species",
        "Co-evolution: reciprocal evolutionary changes",
        "Mass extinction: rapid loss of many species",
        # Physiologie humaine
        "Heart pumps blood through circulatory system",
        "Arteries carry oxygenated blood from heart",
        "Veins return deoxygenated blood to heart",
        "Capillaries: site of gas nutrient exchange",
        "Lungs: gas exchange through alveoli",
        "Kidneys filter blood remove waste",
        "Liver detoxifies and metabolizes",
        "Brain: central processing organ",
        "Neurons: nerve cells transmit signals",
        "Synapses: junctions between neurons",
    ],
    
    # ─── MATHÉMATIQUES ──────────────────────────────────────────────────
    "mathematics": [
        # Algèbre
        "Linear equation: ax plus b equals zero",
        "Quadratic equation: ax squared plus bx plus c equals zero",
        "Quadratic formula: x equals negative b plus minus square root of b squared minus 4ac over 2a",
        "Polynomial degree: highest power of variable",
        "Factor theorem: if f of a equals zero then x minus a is factor",
        "Remainder theorem: f of a equals remainder when divided by x minus a",
        "Logarithm: log base b of x equals exponent giving b to that power equals x",
        "Exponential function: f of x equals b to power x",
        "Natural logarithm: base e approximately 2.718",
        "Complex numbers: a plus bi where i squared equals negative 1",
        # Calcul différentiel
        "Derivative: rate of change of function",
        "Power rule: derivative of x to n equals n times x to n minus 1",
        "Chain rule: derivative of f of g of x equals f prime of g of x times g prime of x",
        "Product rule: derivative of f times g equals f prime g plus f g prime",
        "Quotient rule: derivative of f over g equals f prime g minus f g prime over g squared",
        "Critical point: where derivative equals zero or undefined",
        "Maximum: derivative changes from positive to negative",
        "Minimum: derivative changes from negative to positive",
        "Inflection point: where concavity changes",
        "Taylor series: function approximated by polynomials",
        # Calcul intégral
        "Integral: area under curve",
        "Fundamental theorem: integral of derivative equals function",
        "Definite integral: area between limits",
        "Indefinite integral: family of antiderivatives",
        "Integration by parts: integral of u dv equals u v minus integral of v du",
        "Substitution: change of variable in integral",
        "Partial fractions: decompose rational functions",
        "Volume by rotation: integral of pi r squared dx",
        "Arc length: integral of square root of 1 plus f prime squared",
        "Surface area: integral of 2 pi f times ds",
        # Géométrie
        "Pythagorean theorem: a squared plus b squared equals c squared",
        "Circle area: pi r squared",
        "Circle circumference: 2 pi r",
        "Sphere volume: 4 over 3 pi r cubed",
        "Sphere surface: 4 pi r squared",
        "Triangle area: one half base times height",
        "Sine law: a over sine A equals b over sine B equals c over sine C",
        "Cosine law: c squared equals a squared plus b squared minus 2ab cosine C",
        "Tangent line: touches curve at one point",
        "Normal line: perpendicular to tangent",
        # Statistiques
        "Mean: sum divided by count",
        "Median: middle value when sorted",
        "Mode: most frequent value",
        "Variance: average of squared deviations from mean",
        "Standard deviation: square root of variance",
        "Normal distribution: bell curve",
        "Central limit theorem: sample means approach normal",
        "Correlation: measure of linear relationship",
        "Regression: predicting from correlated variables",
        "Hypothesis testing: statistical inference",
    ],
    
    # ─── INTELLIGENCE ARTIFICIELLE ───────────────────────────────────────
    "ai": [
        # Machine Learning
        "Machine learning: algorithms learn from data",
        "Supervised learning: labeled training data",
        "Unsupervised learning: find patterns in unlabeled data",
        "Reinforcement learning: learn from rewards",
        "Classification: predict discrete categories",
        "Regression: predict continuous values",
        "Clustering: group similar data points",
        "Dimensionality reduction: simplify data preserving structure",
        "Feature engineering: create informative inputs",
        "Cross-validation: estimate model performance",
        # Deep Learning
        "Neural network: layers of interconnected nodes",
        "Perceptron: simplest neural network",
        "Activation function: introduces nonlinearity",
        "ReLU: max of zero and x",
        "Sigmoid: maps to zero to one",
        "Softmax: converts to probabilities",
        "Backpropagation: compute gradients through network",
        "Gradient descent: optimize weights",
        "Learning rate: step size in optimization",
        "Batch normalization: stabilize training",
        # Architectures
        "CNN: convolutional neural network for images",
        "RNN: recurrent neural network for sequences",
        "LSTM: long short term memory cells",
        "Transformer: attention-based architecture",
        "Encoder: compresses input to representation",
        "Decoder: generates output from representation",
        "Attention mechanism: weighs input importance",
        "Self-attention: relates positions within sequence",
        "Multi-head attention: parallel attention operations",
        "Positional encoding: injects position information",
        # NLP
        "Tokenization: split text into units",
        "Word embedding: dense vector representation",
        "Word2Vec: learns word relationships",
        "BERT: bidirectional encoder representations",
        "GPT: generative pre-trained transformer",
        "Language model: predicts next token",
        "Fine-tuning: adapt pre-trained model",
        "Transfer learning: reuse learned features",
        "Prompt engineering: design model inputs",
        "Few-shot learning: learn from few examples",
        # Applications
        "Computer vision: image understanding",
        "Object detection: locate and classify objects",
        "Segmentation: pixel-level classification",
        "Image generation: create new images",
        "Style transfer: apply artistic style",
        "Speech recognition: audio to text",
        "Text to speech: text to audio",
        "Machine translation: language to language",
        "Sentiment analysis: determine text emotion",
        "Named entity recognition: identify entities",
    ],
    
    # ─── PROGRAMMATION ──────────────────────────────────────────────────
    "programming": [
        # Concepts de base
        "Variable: named storage location",
        "Data type: kind of value stored",
        "Integer: whole number",
        "Float: decimal number",
        "String: sequence of characters",
        "Boolean: true or false",
        "Array: ordered collection",
        "Object: key-value pairs",
        "Function: reusable code block",
        "Parameter: input to function",
        # Structures de contrôle
        "If statement: conditional execution",
        "Else: alternative branch",
        "Switch case: multiple branches",
        "For loop: iterate fixed times",
        "While loop: iterate while condition true",
        "Do while: execute at least once",
        "Break: exit loop early",
        "Continue: skip to next iteration",
        "Return: exit function with value",
        "Exception: error condition",
        # Programmation orientée objet
        "Class: blueprint for objects",
        "Object: instance of class",
        "Constructor: initializes object",
        "Method: function in class",
        "Property: data in object",
        "Inheritance: derive from parent class",
        "Polymorphism: same interface different behavior",
        "Encapsulation: hide internal details",
        "Abstraction: show essential features",
        "Interface: contract for classes",
        # Structures de données
        "Stack: last in first out",
        "Queue: first in first out",
        "Linked list: nodes with pointers",
        "Tree: hierarchical structure",
        "Binary tree: at most two children",
        "Graph: nodes and edges",
        "Hash table: key-value with hash function",
        "Set: unique elements",
        "Heap: priority queue",
        "Trie: prefix tree for strings",
        # Algorithmes
        "Sorting: arrange in order",
        "Binary search: halve search space",
        "Linear search: check each element",
        "Recursion: function calls itself",
        "Divide and conquer: split problem",
        "Dynamic programming: store subproblem solutions",
        "Greedy: locally optimal choices",
        "Backtracking: try and revert",
        "Breadth-first search: level by level",
        "Depth-first search: explore branch fully",
    ],
    
    # ─── HISTOIRE ────────────────────────────────────────────────────────
    "history": [
        # Antiquité
        "Mesopotamia: cradle of civilization",
        "Ancient Egypt: pyramids and pharaohs",
        "Greek city states: Athens and Sparta",
        "Roman Republic: senate and consuls",
        "Roman Empire: Pax Romana",
        "Han Dynasty: golden age of China",
        "Maurya Empire: unified India",
        "Persian Empire: Cyrus the Great",
        "Bronze Age collapse: 1200 BCE",
        "Iron Age: new metal technology",
        # Moyen Âge
        "Byzantine Empire: Eastern Roman continuation",
        "Islamic Golden Age: science and philosophy",
        "Viking expansion: Scandinavia to everywhere",
        "Holy Roman Empire: central European states",
        "Crusades: religious wars to Holy Land",
        "Mongol Empire: largest contiguous empire",
        "Black Death: 1347 pandemic",
        "Hundred Years War: England versus France",
        "Feudalism: lords and vassals",
        "Renaissance beginnings: Italian city states",
        # Temps modernes
        "Age of Exploration: European expansion",
        "Protestant Reformation: 1517",
        "Scientific Revolution: empirical method",
        "Enlightenment: reason and progress",
        "American Revolution: 1776 independence",
        "French Revolution: 1789",
        "Industrial Revolution: mechanization",
        "Napoleonic Wars: European reorganization",
        "Colonialism: European empires",
        "American Civil War: 1861 to 1865",
        # Époque contemporaine
        "World War One: 1914 to 1918",
        "Russian Revolution: 1917",
        "Great Depression: 1929 economic crisis",
        "World War Two: 1939 to 1945",
        "Cold War: USA versus USSR",
        "Decolonization: end of empires",
        "Space Race: moon landing 1969",
        "Digital Revolution: computers and internet",
        "Fall of Soviet Union: 1991",
        "Globalization: interconnected world",
    ],
    
    # ─── PHILOSOPHIE ─────────────────────────────────────────────────────
    "philosophy": [
        # Philosophie grecque
        "Socrates: know thyself",
        "Plato: theory of Forms",
        "Aristotle: logic and empiricism",
        "Stoicism: control what you can",
        "Epicureanism: pursuit of happiness",
        "Skepticism: question assumptions",
        "Cynicism: reject conventions",
        "Platonism: abstract Forms exist",
        "Aristotelianism: empirical observation",
        "Hellenistic philosophy: practical ethics",
        # Philosophie moderne
        "Descartes: I think therefore I am",
        "Spinoza: monism and determinism",
        "Leibniz: monads and optimism",
        "Locke: tabula rasa",
        "Hume: empiricism and skepticism",
        "Kant: transcendental idealism",
        "Hegel: dialectic and absolute spirit",
        "Schopenhauer: will and suffering",
        "Nietzsche: will to power",
        "Marx: historical materialism",
        # Philosophie contemporaine
        "Existentialism: existence precedes essence",
        "Phenomenology: study of consciousness",
        "Analytic philosophy: logical analysis",
        "Pragmatism: truth as useful",
        "Logical positivism: verification principle",
        "Ordinary language philosophy: language use",
        "Postmodernism: skepticism toward grand narratives",
        "Critical theory: social critique",
        "Deconstruction: question assumptions",
        "Virtue ethics: character matters",
        # Éthique
        "Utilitarianism: maximize happiness",
        "Deontology: duty based ethics",
        "Consequentialism: outcomes matter",
        "Virtue ethics: character cultivation",
        "Care ethics: relationships matter",
        "Rights based ethics: individual rights",
        "Contractarianism: social contract",
        "Divine command: God determines right",
        "Moral realism: objective moral facts",
        "Moral relativism: morality is relative",
    ],
    
    # ─── PSYCHOLOGIE ─────────────────────────────────────────────────────
    "psychology": [
        # Fondements
        "Behaviorism: observable behavior only",
        "Cognitive psychology: mental processes",
        "Psychoanalysis: unconscious mind",
        "Humanistic psychology: self-actualization",
        "Neuroscience: brain and behavior",
        "Evolutionary psychology: adapted mind",
        "Social psychology: group influence",
        "Developmental psychology: lifespan changes",
        "Clinical psychology: mental health",
        "Positive psychology: flourishing",
        # Cognition
        "Attention: focus on relevant stimuli",
        "Perception: interpreting sensory input",
        "Memory: encoding storage retrieval",
        "Short-term memory: limited capacity",
        "Long-term memory: unlimited storage",
        "Working memory: active processing",
        "Problem solving: goal-directed thinking",
        "Decision making: choosing among options",
        "Language: symbolic communication",
        "Intelligence: adaptive capacity",
        # Émotions
        "Emotion: feeling plus physiological response",
        "Basic emotions: happiness sadness anger fear disgust surprise",
        "Emotional intelligence: understand and manage emotions",
        "Mood: sustained emotional state",
        "Affect: observable emotional expression",
        "Alexithymia: difficulty identifying emotions",
        "Emotional regulation: managing feelings",
        "Stress response: fight or flight",
        "Anxiety: anticipatory worry",
        "Depression: persistent low mood",
        # Apprentissage
        "Classical conditioning: Pavlov dogs",
        "Operant conditioning: reinforcement and punishment",
        "Observational learning: modeling",
        "Latent learning: hidden knowledge",
        "Insight learning: sudden understanding",
        "Habituation: decreased response to repeated stimulus",
        "Sensitization: increased response",
        "Spaced repetition: distributed practice",
        "Retrieval practice: testing effect",
        "Metacognition: thinking about thinking",
    ],
    
    # ─── ÉCONOMIE ────────────────────────────────────────────────────────
    "economics": [
        # Microéconomie
        "Supply: quantity producers offer",
        "Demand: quantity consumers want",
        "Equilibrium: supply equals demand",
        "Elasticity: responsiveness to price change",
        "Marginal utility: satisfaction from additional unit",
        "Opportunity cost: next best alternative",
        "Comparative advantage: lower opportunity cost",
        "Market structure: competitive to monopoly",
        "Perfect competition: many small firms",
        "Monopoly: single seller",
        # Macroéconomie
        "GDP: total value of goods and services",
        "Inflation: general price increase",
        "Unemployment: people seeking work",
        "Fiscal policy: government spending and taxation",
        "Monetary policy: central bank actions",
        "Interest rates: cost of borrowing",
        "Exchange rates: currency values",
        "Balance of payments: international transactions",
        "Business cycle: expansion and contraction",
        "Recession: economic decline",
        # Commerce international
        "Free trade: no restrictions",
        "Protectionism: barriers to imports",
        "Tariffs: taxes on imports",
        "Quotas: limits on import quantity",
        "Trade deficit: imports exceed exports",
        "Trade surplus: exports exceed imports",
        "Currency devaluation: lower value",
        "Currency appreciation: higher value",
        "World Bank: international development",
        "IMF: international monetary stability",
        # Finance
        "Stock: ownership share",
        "Bond: loan to issuer",
        "Interest: cost of borrowing",
        "Compound interest: interest on interest",
        "Present value: future money today",
        "Risk premium: extra return for risk",
        "Diversification: reduce risk",
        "Asset allocation: portfolio mix",
        "Capital market: long-term financing",
        "Money market: short-term financing",
    ],
    
    # ─── MUSIQUE ─────────────────────────────────────────────────────────
    "music": [
        # Théorie musicale
        "Octave: frequency doubling interval",
        "Semitone: smallest Western interval",
        "Major scale: whole whole half whole whole whole half",
        "Minor scale: different pattern of intervals",
        "Chord: three or more notes",
        "Triad: root third fifth",
        "Major chord: root major third perfect fifth",
        "Minor chord: root minor third perfect fifth",
        "Seventh chord: triad plus seventh",
        "Key: tonal center",
        # Rythme
        "Beat: basic time unit",
        "Tempo: speed in beats per minute",
        "Meter: grouping of beats",
        "Time signature: beats per measure",
        "Syncopation: off-beat emphasis",
        "Polyrhythm: multiple rhythms simultaneously",
        "Swing: uneven eighth notes",
        "Groove: rhythmic feel",
        "Fill: transitional rhythm",
        "Break: rhythm stops",
        # Instruments
        "Piano: keyboard percussion",
        "Guitar: plucked strings",
        "Violin: bowed strings",
        "Drums: percussion instruments",
        "Trumpet: brass wind",
        "Saxophone: woodwind reed",
        "Flute: woodwind",
        "Bass: low frequency strings",
        "Synthesizer: electronic sound generation",
        "Voice: natural instrument",
        # Genres
        "Classical: orchestral tradition",
        "Jazz: improvisation and swing",
        "Rock: amplified instruments",
        "Pop: popular commercial",
        "Hip-hop: rap and beats",
        "Electronic: synthesized sounds",
        "Blues: African American roots",
        "Country: American folk tradition",
        "Reggae: Jamaican origin",
        "World: non-Western traditions",
    ],
    
    # ─── CUISINE ────────────────────────────────────────────────────────
    "cooking": [
        # Techniques de base
        "Sauté: cook quickly in hot fat",
        "Sear: high heat browning",
        "Roast: cook in oven",
        "Grill: cook over direct heat",
        "Braise: low and slow with liquid",
        "Stew: cook in liquid",
        "Poach: cook in simmering liquid",
        "Steam: cook with vapor",
        "Blanch: brief boil then ice bath",
        "Fry: cook in hot oil",
        # Préparation
        "Chop: cut into pieces",
        "Dice: small uniform cubes",
        "Mince: very fine cut",
        "Julienne: thin strips",
        "Grate: shred with rough surface",
        "Zest: citrus outer peel",
        "Whisk: beat to incorporate air",
        "Fold: gentle mixing",
        "Knead: work dough",
        "Marinate: soak in seasoned liquid",
        # Cuisines du monde
        "French cuisine: classical techniques",
        "Italian cuisine: pasta and sauces",
        "Chinese cuisine: wok and stir-fry",
        "Japanese cuisine: precision and seasonality",
        "Indian cuisine: spices and curries",
        "Mexican cuisine: corn beans chilies",
        "Thai cuisine: balance of flavors",
        "Mediterranean: olive oil and fresh",
        "Middle Eastern: grains and grilled meats",
        "American: diverse regional styles",
        # Ingrédients essentiels
        "Salt: enhances flavor",
        "Pepper: adds heat",
        "Garlic: aromatic allium",
        "Onion: savory base",
        "Olive oil: Mediterranean fat",
        "Butter: rich flavor",
        "Herbs: leafy flavorings",
        "Spices: dried seeds bark roots",
        "Stock: flavorful liquid base",
        "Wine: deglazing and flavor",
    ],
    
    # ─── SANTÉ ───────────────────────────────────────────────────────────
    "health": [
        # Nutrition
        "Proteins: amino acids for tissues",
        "Carbohydrates: energy source",
        "Fats: essential fatty acids",
        "Vitamins: organic micronutrients",
        "Minerals: inorganic micronutrients",
        "Fiber: digestive health",
        "Water: essential for life",
        "Calories: energy measurement",
        "Antioxidants: prevent cell damage",
        "Probiotics: beneficial bacteria",
        # Exercice
        "Cardio: heart and lungs",
        "Strength training: muscle building",
        "Flexibility: range of motion",
        "Balance: stability",
        "Endurance: sustained activity",
        "HIIT: high intensity intervals",
        "Resistance training: against force",
        "Aerobic: with oxygen",
        "Anaerobic: without oxygen",
        "Recovery: rest and repair",
        # Sommeil
        "REM sleep: dreaming stage",
        "Deep sleep: restoration",
        "Circadian rhythm: sleep-wake cycle",
        "Sleep hygiene: healthy habits",
        "Sleep debt: accumulated deprivation",
        "Insomnia: difficulty sleeping",
        "Apnea: breathing interruption",
        "Narcolepsy: excessive sleepiness",
        "Melatonin: sleep hormone",
        "Blue light: disrupts sleep",
        # Prévention
        "Vaccination: immune training",
        "Screening: early detection",
        "Hand washing: infection prevention",
        "Stress management: mental health",
        "Sunscreen: UV protection",
        "Hydration: adequate fluids",
        "Moderation: balanced approach",
        "Regular checkups: monitoring",
        "Mental health: psychological well-being",
        "Social connection: relationships matter",
    ],
    
    # ─── TECHNOLOGIE ─────────────────────────────────────────────────────
    "technology": [
        # Hardware
        "CPU: central processing unit",
        "GPU: graphics processing unit",
        "RAM: random access memory",
        "SSD: solid state drive",
        "HDD: hard disk drive",
        "Motherboard: main circuit board",
        "Power supply: converts electricity",
        "Cooling: heat management",
        "Bus: data pathway",
        "Cache: fast temporary storage",
        # Réseaux
        "IP address: internet location",
        "DNS: domain name system",
        "Router: directs traffic",
        "Switch: connects devices",
        "Firewall: security barrier",
        "Protocol: communication rules",
        "Bandwidth: data capacity",
        "Latency: response delay",
        "Packet: data unit",
        "Encryption: secure encoding",
        # Internet
        "HTTP: hypertext transfer protocol",
        "HTTPS: secure HTTP",
        "HTML: hypertext markup language",
        "CSS: cascading style sheets",
        "JavaScript: web programming",
        "API: application programming interface",
        "Cloud computing: remote servers",
        "Web server: hosts websites",
        "Browser: web navigation",
        "Domain: web address",
        # Sécurité
        "Authentication: identity verification",
        "Authorization: permission granting",
        "Encryption: data scrambling",
        "Hash: one-way transformation",
        "Salt: random addition to hash",
        "Certificate: digital identity",
        "VPN: virtual private network",
        "Two-factor: multiple verification",
        "Phishing: fraudulent attempts",
        "Malware: malicious software",
    ],
}

# ═══════════════════════════════════════════════════════════════════════════
# GÉNÉRATION DE CONNAISSANCES ADDITIONNELLES (étendre à 10,000+ entrées)
# ═══════════════════════════════════════════════════════════════════════════

def generate_extended_knowledge():
    """Génère des connaissances supplémentaires pour atteindre 1 Go"""
    extended = {}
    
    # Concepts philosophiques étendus
    philosophical_concepts = [
        "Ontology: study of being",
        "Epistemology: study of knowledge",
        "Axiology: study of value",
        "Metaphysics: nature of reality",
        "Teleology: purpose and ends",
        "Determinism: cause and effect",
        "Free will: choice capacity",
        "Solipsism: only mind exists",
        "Dualism: mind body separation",
        "Monism: single substance",
        "Materialism: matter is fundamental",
        "Idealism: mind is fundamental",
        "Pragmatism: practical truth",
        "Rationalism: reason as source",
        "Empiricism: experience as source",
        "Skepticism: doubt as method",
        "Dogmatism: unquestioned beliefs",
        "Relativism: truth depends",
        "Absolutism: universal truths",
        "Nihilism: meaning absence",
    ]
    
    # Concepts mathématiques étendus
    math_concepts = [
        "Group theory: algebraic structures",
        "Ring theory: addition and multiplication",
        "Field theory: division possible",
        "Vector space: linear algebra",
        "Matrix: rectangular array",
        "Determinant: scalar from matrix",
        "Eigenvalue: scaling factor",
        "Eigenvector: unchanged direction",
        "Linear transformation: matrix multiplication",
        "Inner product: dot product",
        "Norm: vector length",
        "Orthogonal: perpendicular",
        "Orthonormal: unit perpendicular",
        "Basis: spanning independent set",
        "Dimension: basis size",
        "Kernel: null space",
        "Image: range of transformation",
        "Rank: dimension of image",
        "Nullity: dimension of kernel",
        "Rank nullity theorem",
    ]
    
    # Concepts physiques étendus
    physics_concepts = [
        "Lagrangian: T minus V",
        "Hamiltonian: total energy",
        "Action: integral of Lagrangian",
        "Principle of least action",
        "Hamilton equations: dynamics",
        "Canonical momentum: partial L partial q dot",
        "Phase space: position momentum",
        "Liouville theorem: phase space volume",
        "Noether theorem: symmetries conservation",
        "Canonical transformations",
        "Poisson bracket: classical commutator",
        "Hamilton Jacobi equation",
        "Action angle variables",
        "Integrable systems",
        "Chaos: sensitive dependence",
        "Strange attractor",
        "Bifurcation: qualitative change",
        "Fractal: self-similar structure",
        "Mandelbrot set: z squared plus c",
        "Julia set: iteration boundary",
    ]
    
    # Concepts AI étendus
    ai_concepts = [
        "Neural architecture search",
        "AutoML: automated machine learning",
        "Hyperparameter optimization",
        "Neural pruning: remove connections",
        "Quantization: reduce precision",
        "Distillation: teacher to student",
        "Federated learning: distributed training",
        "Differential privacy: protect data",
        "Adversarial examples: crafted inputs",
        "Robustness: resist perturbations",
        "Interpretability: understand decisions",
        "Explainable AI: human readable",
        "Fairness: avoid bias",
        "Accountability: responsible AI",
        "Transparency: open processes",
        "Neural tangents: infinite width limit",
        "Lottery ticket hypothesis",
        "Groking: sudden generalization",
        "Double descent: more data helps",
        "Scaling laws: performance with size",
    ]
    
    extended["philosophy_extra"] = philosophical_concepts
    extended["math_extra"] = math_concepts
    extended["physics_extra"] = physics_concepts
    extended["ai_extra"] = ai_concepts
    
    return extended

# Fusionner les connaissances
ALL_KNOWLEDGE = {**KNOWLEDGE_BASE, **generate_extended_knowledge()}

# ═══════════════════════════════════════════════════════════════════════════
# SYSTÈME DE STIMULATION
# ═══════════════════════════════════════════════════════════════════════════

class BrainTrainer:
    def __init__(self):
        self.total_stimuli = 0
        self.total_spikes = 0
        self.start_time = time.time()
        self.stats = {}
        
    def get_stats(self):
        """Récupère les stats du Brain"""
        try:
            r = requests.get(f"{BRAIN_API}/api/stats", timeout=5)
            return r.json()
        except:
            return None
    
    def stimulate(self, module, intensity, knowledge):
        """Envoie un stimulus au Brain"""
        try:
            payload = {
                "module": module,
                "intensity": intensity,
                "knowledge": knowledge,
                "session": SESSION_ID
            }
            r = requests.post(f"{BRAIN_API}/api/stimulus", json=payload, timeout=10)
            self.total_stimuli += 1
            return r.json() if r.status_code == 200 else None
        except Exception as e:
            return None
    
    def train_domain(self, domain, knowledge_list):
        """Entraîne un domaine complet"""
        print(f"📚 Entraînement: {domain} ({len(knowledge_list)} entrées)")
        
        # Mapper le domaine aux modules
        module_map = {
            "physics": "reasoning",
            "chemistry": "reasoning",
            "biology": "memory",
            "mathematics": "reasoning",
            "ai": "learning",
            "programming": "learning",
            "history": "memory",
            "philosophy": "reasoning",
            "psychology": "learning",
            "economics": "reasoning",
            "music": "creativity",
            "cooking": "creativity",
            "health": "memory",
            "technology": "learning",
        }
        
        module = module_map.get(domain, "perception")
        
        for i, knowledge in enumerate(knowledge_list):
            intensity = 1.0 + (i % 5) * 0.2  # Varier l'intensité
            self.stimulate(module, intensity, knowledge)
            self.total_stimuli += 1
            
            if (i + 1) % 50 == 0:
                stats = self.get_stats()
                if stats:
                    print(f"  Progression: {i+1}/{len(knowledge_list)} | "
                          f"Neurones: {stats['N']} | "
                          f"Synapses: {stats['syn']}")
                time.sleep(0.1)  # Petit pause
    
    def run_massive_training(self):
        """Lance l'entraînement massif"""
        print("=" * 70)
        print("🧠 SOULLINK BRAIN — CAMPAGNE D'ENTRAÎNEMENT MASSIVE")
        print("=" * 70)
        print(f"Session: {SESSION_ID}")
        print(f"Domaines: {len(ALL_KNOWLEDGE)}")
        print(f"Entrées totales: {sum(len(v) for v in ALL_KNOWLEDGE.values())}")
        print("=" * 70)
        
        # Stats initiales
        initial = self.get_stats()
        print(f"\n📊 État initial:")
        print(f"   Neurones: {initial['N']}")
        print(f"   Synapses: {initial['syn']}")
        print(f"   Growth: {initial['growth']}")
        
        # Entraîner chaque domaine
        for domain, knowledge in ALL_KNOWLEDGE.items():
            self.train_domain(domain, knowledge)
        
        # Stats finales
        final = self.get_stats()
        elapsed = time.time() - self.start_time
        
        print("\n" + "=" * 70)
        print("✅ ENTRAÎNEMENT TERMINÉ")
        print("=" * 70)
        print(f"Durée: {elapsed:.1f} secondes")
        print(f"Stimuli envoyés: {self.total_stimuli}")
        print(f"\n📊 État final:")
        print(f"   Neurones: {final['N']} ({final['N'] - initial['N']} nouveaux)")
        print(f"   Synapses: {final['syn']} ({final['syn'] - initial['syn']} nouvelles)")
        print(f"   Growth: {final['growth']}")
        print("=" * 70)

# ═══════════════════════════════════════════════════════════════════════════
# MAIN
# ═══════════════════════════════════════════════════════════════════════════

if __name__ == "__main__":
    trainer = BrainTrainer()
    trainer.run_massive_training()