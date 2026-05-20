import { lazy, Suspense } from 'react';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';

const MonacoWrapper = lazy(() =>
  import('@/components/editor/MonacoWrapper').then((m) => ({ default: m.MonacoWrapper })),
);

interface ScriptsTabProps {
  preRequestScript: string;
  postResponseScript: string;
  testsScript: string;
  onChangePreRequest: (value: string) => void;
  onChangePostResponse: (value: string) => void;
  onChangeTests: (value: string) => void;
  readOnly?: boolean;
}

export function ScriptsTab({
  preRequestScript,
  postResponseScript,
  testsScript,
  onChangePreRequest,
  onChangePostResponse,
  onChangeTests,
  readOnly = false,
}: ScriptsTabProps) {
  return (
    <Tabs defaultValue='pre-request' className='flex flex-col h-full'>
      <TabsList className='shrink-0 w-full justify-start rounded-none border-b bg-transparent px-2'>
        <TabsTrigger value='pre-request' className='text-xs'>
          Pre Request
        </TabsTrigger>
        <TabsTrigger value='post-response' className='text-xs'>
          Post Response
        </TabsTrigger>
        <TabsTrigger value='tests' className='text-xs'>
          Tests
        </TabsTrigger>
      </TabsList>

      <TabsContent value='pre-request' className='flex-1 m-0 p-0'>
        <Suspense fallback={null}>
          <MonacoWrapper
            language='javascript'
            value={preRequestScript}
            onChange={readOnly ? undefined : onChangePreRequest}
            readOnly={readOnly}
            height='100%'
          />
        </Suspense>
      </TabsContent>

      <TabsContent value='post-response' className='flex-1 m-0 p-0'>
        <Suspense fallback={null}>
          <MonacoWrapper
            language='javascript'
            value={postResponseScript}
            onChange={readOnly ? undefined : onChangePostResponse}
            readOnly={readOnly}
            height='100%'
          />
        </Suspense>
      </TabsContent>

      <TabsContent value='tests' className='flex-1 m-0 p-0'>
        <Suspense fallback={null}>
          <MonacoWrapper
            language='javascript'
            value={testsScript}
            onChange={readOnly ? undefined : onChangeTests}
            readOnly={readOnly}
            height='100%'
          />
        </Suspense>
      </TabsContent>
    </Tabs>
  );
}
