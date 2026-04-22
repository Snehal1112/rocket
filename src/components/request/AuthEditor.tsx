import { Key, Lock, User } from 'lucide-react';
import { useCallback } from 'react';
import { SingleLineEditor } from '@/components/editor';
import { Card, CardContent } from '@/components/ui/card';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { VariableScopeEntry, VariableSource } from '@/lib/url-variables';
import type { AuthState } from '@/types/pane-types';
import { OAuth2AuthEditor } from './oauth2/OAuth2AuthEditor';

interface AuthEditorProps {
  auth: AuthState;
  onChange: (auth: AuthState) => void;
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource | 'pathParam', key: string) => void;
  collection?: string;
  environmentName?: string;
  requestPath?: string;
}

export function AuthEditor({
  auth,
  onChange,
  variableContext,
  onNavigateToSource,
  collection,
  environmentName,
  requestPath,
}: AuthEditorProps) {
  const patchOAuth2 = useCallback(
    (patch: Partial<NonNullable<AuthState['oauth2']>>) => {
      onChange({
        ...auth,
        oauth2: { ...auth.oauth2, ...patch } as NonNullable<AuthState['oauth2']>,
      });
    },
    [auth, onChange],
  );

  const patchAWS = useCallback(
    (patch: Partial<NonNullable<AuthState['awsSigV4']>>) => {
      onChange({
        ...auth,
        awsSigV4: { ...auth.awsSigV4, ...patch } as NonNullable<AuthState['awsSigV4']>,
      });
    },
    [auth, onChange],
  );

  return (
    <div className='space-y-4'>
      {auth.authType === 'inherit' && (
        <Card className='bg-muted/50'>
          <CardContent className='px-3 py-2.5'>
            <p className='text-xs text-muted-foreground'>
              This request inherits authorization from the collection settings. To override, select
              a different auth type above.
            </p>
          </CardContent>
        </Card>
      )}

      {auth.authType === 'none' && (
        <p className='text-xs text-muted-foreground'>No authentication configured.</p>
      )}

      {auth.authType === 'basic' && auth.basic && (
        <Card>
          <CardContent className='space-y-2 p-4'>
            <div className='flex items-center gap-2'>
              <User className='h-3.5 w-3.5 text-muted-foreground' />
              <SingleLineEditor
                placeholder='Username'
                aria-label='Username'
                className='flex-1 text-sm'
                value={auth.basic.username}
                onChange={(newVal) =>
                  onChange({
                    ...auth,
                    basic: { ...auth.basic, username: newVal } as NonNullable<AuthState['basic']>,
                  })
                }
                variableContext={variableContext}
                onNavigateToSource={onNavigateToSource}
              />
            </div>
            <div className='flex items-center gap-2'>
              <Lock className='h-3.5 w-3.5 text-muted-foreground' />
              <SingleLineEditor
                placeholder='Password'
                aria-label='Password'
                isSecret
                className='flex-1 text-sm'
                value={auth.basic.password}
                onChange={(newVal) =>
                  onChange({
                    ...auth,
                    basic: { ...auth.basic, password: newVal } as NonNullable<AuthState['basic']>,
                  })
                }
                variableContext={variableContext}
                onNavigateToSource={onNavigateToSource}
              />
            </div>
          </CardContent>
        </Card>
      )}

      {auth.authType === 'bearer' && auth.bearer && (
        <Card>
          <CardContent className='flex items-center gap-2 p-4'>
            <Key className='h-3.5 w-3.5 text-muted-foreground' />
            <SingleLineEditor
              placeholder='Token'
              aria-label='Bearer token'
              isSecret
              className='flex-1 text-sm'
              value={auth.bearer.token}
              onChange={(newVal) =>
                onChange({
                  ...auth,
                  bearer: { token: newVal },
                })
              }
              variableContext={variableContext}
              onNavigateToSource={onNavigateToSource}
            />
          </CardContent>
        </Card>
      )}

      {auth.authType === 'api-key' && auth.apiKey && (
        <Card>
          <CardContent className='space-y-2 p-4'>
            <SingleLineEditor
              placeholder='Key'
              aria-label='API key name'
              className='text-sm'
              value={auth.apiKey.key}
              onChange={(newVal) =>
                onChange({
                  ...auth,
                  apiKey: { ...auth.apiKey, key: newVal } as NonNullable<AuthState['apiKey']>,
                })
              }
              variableContext={variableContext}
              onNavigateToSource={onNavigateToSource}
            />
            <SingleLineEditor
              placeholder='Value'
              aria-label='API key value'
              isSecret
              className='text-sm'
              value={auth.apiKey.value}
              onChange={(newVal) =>
                onChange({
                  ...auth,
                  apiKey: { ...auth.apiKey, value: newVal } as NonNullable<AuthState['apiKey']>,
                })
              }
              variableContext={variableContext}
              onNavigateToSource={onNavigateToSource}
            />
            <Select
              value={auth.apiKey.addTo}
              onValueChange={(val) =>
                onChange({
                  ...auth,
                  apiKey: {
                    ...auth.apiKey,
                    addTo: val as 'header' | 'query',
                  } as NonNullable<AuthState['apiKey']>,
                })
              }
            >
              <SelectTrigger className='w-30 text-sm'>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value='header' className='text-sm'>
                  Header
                </SelectItem>
                <SelectItem value='query' className='text-sm'>
                  Query Param
                </SelectItem>
              </SelectContent>
            </Select>
          </CardContent>
        </Card>
      )}

      {auth.authType === 'oauth2' && auth.oauth2 && (
        <OAuth2AuthEditor
          oauth2={auth.oauth2}
          patchOAuth2={patchOAuth2}
          variableContext={variableContext}
          onNavigateToSource={onNavigateToSource}
          collection={collection}
          environmentName={environmentName}
          requestPath={requestPath}
        />
      )}

      {auth.authType === 'aws-sig-v4' && auth.awsSigV4 && (
        <>
          <Card>
            <CardContent className='space-y-3 p-4'>
              <p className='text-xs font-medium text-muted-foreground'>Credentials</p>
              <div>
                <Label className='mb-1 block'>Access Key</Label>
                <SingleLineEditor
                  className='text-sm'
                  placeholder='AKIAIOSFODNN7EXAMPLE'
                  value={auth.awsSigV4.accessKey}
                  onChange={(newVal) => patchAWS({ accessKey: newVal })}
                  variableContext={variableContext}
                  onNavigateToSource={onNavigateToSource}
                />
              </div>

              <div>
                <Label className='mb-1 block'>Secret Key</Label>
                <SingleLineEditor
                  className='text-sm'
                  isSecret
                  placeholder='wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY'
                  value={auth.awsSigV4.secretKey}
                  onChange={(newVal) => patchAWS({ secretKey: newVal })}
                  variableContext={variableContext}
                  onNavigateToSource={onNavigateToSource}
                />
              </div>

              <div>
                <Label className='mb-1 block'>Session Token</Label>
                <SingleLineEditor
                  className='text-sm'
                  isSecret
                  placeholder='(optional)'
                  value={auth.awsSigV4.sessionToken}
                  onChange={(newVal) => patchAWS({ sessionToken: newVal })}
                  variableContext={variableContext}
                  onNavigateToSource={onNavigateToSource}
                />
                <p className='mt-1 text-xs text-muted-foreground'>
                  Only required for temporary credentials.
                </p>
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardContent className='space-y-3 p-4'>
              <p className='text-xs font-medium text-muted-foreground'>Request Signing</p>
              <div className='grid grid-cols-2 gap-2'>
                <div>
                  <Label className='mb-1 block'>Region</Label>
                  <SingleLineEditor
                    className='text-sm'
                    placeholder='us-east-1'
                    value={auth.awsSigV4.region}
                    onChange={(newVal) => patchAWS({ region: newVal })}
                    variableContext={variableContext}
                    onNavigateToSource={onNavigateToSource}
                  />
                </div>
                <div>
                  <Label className='mb-1 block'>Service</Label>
                  <SingleLineEditor
                    className='text-sm'
                    placeholder='execute-api'
                    value={auth.awsSigV4.service}
                    onChange={(newVal) => patchAWS({ service: newVal })}
                    variableContext={variableContext}
                    onNavigateToSource={onNavigateToSource}
                  />
                </div>
              </div>
            </CardContent>
          </Card>
        </>
      )}
    </div>
  );
}
