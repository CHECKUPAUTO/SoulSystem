# Code Signing

Tout code exécuté (OpenEvolve, AVID) doit être signé.

## Clés autorisées

```bash
soulsystem key add <public_key_base64>
```

Les clés sont stockées dans `~/.soulsystem/authorized_keys`.

## Vérification

Avant exécution, le code source est vérifié :
- La signature correspond à la clé publique
- La clé publique est dans la liste autorisée
