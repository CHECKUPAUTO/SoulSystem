# Sources Scientifiques pour SoulLink — Plan d'Action

## Sources Accessibles (12) ✅
- ArXiv: 9 catégories (cs.AI, cs.LG, cs.CL, cs.CR, cs.CV, cs.RO, stat.ML, cs.NE, cs.SD)
- HAL: Archive française (API JSON)
- Zenodo: Multidisciplinaire (API REST)
- Hacker News: Tech/Startups (RSS)

## Sources Bloquées (9) — Plan d'Action

### 1. bioRxiv / medRxiv
**Problème:** Cloudflare protection, pas de RSS public
**Solution:** Scraping headless avec Playwright
**Code:** `connectors/biorxiv-scraper.js`
**Délai:** ~2h d'implémentation

### 2. Semantic Scholar
**Problème:** Rate limiting agressif
**Solution:** API Key (gratuite)
**URL:** https://www.semanticscholar.org/product/api
**Code:** `connectors/semantic-scholar-connector.js`
**Délai:** ~30min après obtention key

### 3. HuggingFace Papers
**Problème:** Auth requise
**Solution:** API Token (gratuit)
**URL:** https://huggingface.co/settings/tokens
**Code:** `connectors/huggingface-connector.js`
**Délai:** ~30min après obtention token

### 4. OpenReview
**Problème:** Pas de RSS, API complexe
**Solution:** Scraping des venues (ICLR, NeurIPS, ICML)
**Code:** `connectors/openreview-scraper.js`
**Délai:** ~3h

### 5. PapersWithCode
**Problème:** Pas de RSS
**Solution:** API non documentée, scraping nécessaire
**Code:** `connectors/pwc-scraper.js`
**Délai:** ~2h

### 6. Google Scholar
**Problème:** Bloqué par Google (anti-bot)
**Solution:** Non disponible (impossible)
**Alternative:** SerpAPI ($) ou ScholarScraper (risqué)

### 7. Connected Papers
**Problème:** Pas de RSS
**Solution:** API non publique
**Délai:** Non disponible

### 8. Research Rabbit
**Problème:** Pas de RSS
**Solution:** API non publique
**Délai:** Non disponible

### 9. IACR ePrint
**Problème:** RSS cassé (404)
**Solution:** Parsing HTML direct
**Code:** `connectors/iacr-scraper.js`
**Délai:** ~1h

## Recommandation

1. **Court terme:** Ajouter bioRxiv + medRxiv (scraping)
2. **Moyen terme:** Obtenir API keys (Semantic Scholar, HuggingFace)
3. **Long terme:** OpenReview + PapersWithCode (scraping complexe)
4. **Impossible:** Google Scholar, Connected Papers, Research Rabbit

## Impact sur SoulLink

Chaque domaine enrichit l'écosystème:
- Biologie → HNN (neural computation)
- Chimie → Memory (molecular encoding)
- Médecine → Security (privacy, ethics)
- Physique → Orchestrator (energy dynamics)
- Robotique → Tools (autonomous systems)

## Prochaines Étapes

Attendre confirmation de l'utilisateur pour:
1. Implémenter le scraping (bioRxiv, medRxiv, IACR)
2. Demander/obtenir les API keys
3. Tester les nouveaux connecteurs
