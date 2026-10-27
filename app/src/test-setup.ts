// SPDX-License-Identifier: GPL-3.0-or-later
// Préparation commune des tests Vitest.
//
// Sans préférence enregistrée, l'interface suit la langue du système (`navigator.language`,
// que Node expose aussi) : un test qui attend du français échouerait sur une machine
// anglaise (cas réel : la CI GitHub). Les tests partent donc toujours du français, la
// langue de référence ; ceux qui vérifient l'anglais basculent explicitement.
import { beforeEach } from 'vitest';
import { setLanguage } from '$lib/strings';

beforeEach(() => setLanguage('fr'));
