import { describe, expect, it } from 'vitest';
import {
  type CredentialErrors,
  type CredentialFormState,
  EMPTY_CREDENTIAL_FORM,
} from '../form-types';

describe('form-types', () => {
  it('starts a new agent on the secret manager source with nothing filled in', () => {
    const empty: CredentialFormState = EMPTY_CREDENTIAL_FORM;
    expect(empty).toEqual({
      credentialSource: 'vault',
      credentialEnvVar: '',
      sourceEnvVar: '',
      localCredential: '',
      removeSavedCredential: false,
      vaultConnectionId: '',
      vaultName: '',
      vaultSecretId: '',
      vaultSecretName: '',
    });
  });

  it('allows an error message per credential field', () => {
    const errors: CredentialErrors = { credentialEnvVar: 'Bad name.', vaultName: null };
    expect(errors.credentialEnvVar).toBe('Bad name.');
    expect(errors.vaultName).toBeNull();
  });
});
