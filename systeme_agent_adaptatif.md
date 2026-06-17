# SYSTEME MATHEMATIQUE : AGENT AUTONOME ADAPTATIF

## 1. Etat global de l'agent

A_t = (M_t, θ_t, π_t, G_t, B_t, I_t, S_t)

M_t : mémoire interne
θ_t : paramètres cognitifs
π_t : politique d'action
G_t : objectifs
B_t : modèle interne du monde
I_t : initiative interne
S_t : motivation d'apprentissage social


==================================================
1. PERCEPTION ET MODELE INTERNE
==================================================

o_t = Env(t)

z_t = f_θ(o_t, M_t)

L'agent construit une représentation interne du monde.


==================================================
2. PREDICTION DU FUTUR
==================================================

ô_(t+1) = P_θ(z_t, a_t)

Erreur de prédiction :

E_p(t) = ||o_(t+1) - ô_(t+1)||


==================================================
3. ERREUR D'ACTION (RENFORCEMENT)
==================================================

δ_t =
r_t + γQ(s_(t+1),a_(t+1))
- Q(s_t,a_t)

Mise à jour :

Q_new = Q_old + αδ_t


==================================================
4. ERREUR PAR RAPPORT AUX OBJECTIFS
==================================================

E_g(t)=||G_t-R_t||

G_t : objectif attendu
R_t : résultat obtenu


==================================================
5. ERREUR SOCIALE (APPRENDRE DES AUTRES)
==================================================

Pour un ensemble d'agents :

A={a1,a2,...,an}

Trajectoire observée :

τ_j=(s,a,r,s')

Erreur externe :

E_social = ||Résultat_j - Prédiction_j||


==================================================
6. INCITATION A APPRENDRE DES AUTRES
==================================================

Récompense sociale :

R_s =
ΔPerformance
+
Evitement d'échec
+
Gain de connaissance


Valeur d'observation :

Q_social(j)=
P(E_j)
×
Impact(E_j)
×
Apprentissage(E_j)


Choix :

j* = argmax(Q_social(j))


==================================================
7. INCERTITUDE ET CURIOSITE
==================================================

Incertitude :

U_t = H(B_t)

Motivation :

S_t=f(U_t,E_autrui,N_t,V_t)

Curiosité :

C_t =
Erreur de prédiction
×
Valeur information


==================================================
8. INITIATIVE INTERNE
==================================================

I_t=f(U_t,K_t,G_t,D_t)

L'agent génère ses propres objectifs :

G_(t+1)=GenerateGoal(K_t,U_t,G_t)


==================================================
9. ACTION AUTONOME
==================================================

Actions possibles :

a_t ∈
{
agir,
expérimenter,
observer,
chercher
}


Sélection :

a* =
argmax(
Q(a)
+
GainInformation(a)
-
Coût(a)
)


==================================================
10. RECHERCHE EXTERNE ACTIVE
==================================================

Erreur détectée :

E_t → Hypothèse

Hyp_t =
Generate(Erreur,Mémoire,Objectif)


Recherche :

q_t = SearchQuery(Hyp_t)


Données externes :

D={(s,a,r,e)}


Similarité :

Sim =
Sim(E_actuelle,E_externe)


Transfert :

Correction =
f(Similarité,Réussite)


==================================================
11. TEST DE CORRECTION
==================================================

C_i = Correction candidate

Valeur :

V(C_i)=
Probabilité(Succès)
×
Gain
-
Risque


Choix :

C* = argmax(V(C_i))


==================================================
12. ERREUR GLOBALE
==================================================

E_t =

w_p E_p
+
w_r δ_t
+
w_g E_g
+
w_s E_social
+
w_u U_t
+
w_i I_t


==================================================
13. RECOMPENSE GLOBALE
==================================================

R_total =

R_action
+
R_social
+
R_information


R_information =

ΔU
+
ΔPerformance


==================================================
14. MEMOIRE EVOLUTIVE
==================================================

Mémoire :

M_t =
M_self
+
M_social


M_(t+1)=

M_t
+
α_m h(E_t,o_t,a_t)


==================================================
15. ADAPTATION COGNITIVE
==================================================

θ_(t+1)=

θ_t
-
α_θ ∇E_t


==================================================
16. EVOLUTION DE LA POLITIQUE
==================================================

π_(t+1)=

π_t
+
α_π ∇(Q-G-E)


==================================================
17. AUTO-AMELIORATION
==================================================

A_(t+1)=

A_t
+
βΔ(A_t,E_t)


==================================================
18. BOUCLE COMPLETE
==================================================


Observer

↓

Prédire

↓

Agir

↓

Comparer au réel

↓

Calculer erreurs :

E_self
+
E_social
+
E_monde

↓

Chercher des expériences externes

↓

Créer corrections candidates

↓

Tester

↓

Récompense

↓

Modifier mémoire

↓

Modifier modèle

↓

Modifier stratégie

↓

Nouvelle itération


==================================================
FORMULE GENERALE FINALE
==================================================


Agent_(t+1)

=

Agent_t

+

Apprentissage(

Erreur_soi

+

Erreur_autrui

+

Erreur_monde

+

Réduction_incertitude

)


FIN
