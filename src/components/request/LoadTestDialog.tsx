import { useState } from 'react';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Loader2, Zap } from 'lucide-react';
import { runLoadTest, type LoadTestResult } from '@/lib/tauri-api';
import { toApiAuth, toApiBody } from '@/lib/execute-request';
import type { RequestState } from '@/types/pane-types';

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  request: RequestState;
}

const CONCURRENCY_OPTIONS = ['1', '5', '10', '25', '50', '100'];

export function LoadTestDialog({ open, onOpenChange, request }: Props) {
  const [concurrency, setConcurrency] = useState('10');
  const [totalRequests, setTotalRequests] = useState('100');
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<LoadTestResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handleRun = async () => {
    setRunning(true);
    setResult(null);
    setError(null);
    try {
      // Map RequestState to the shape expected by the Tauri command, reusing
      // the same conversion helpers used in execute-request.ts.
      const res = await runLoadTest(
        {
          method: request.method,
          url: request.url,
          headers: request.headers
            .filter((h) => h.enabled)
            .map((h) => ({ key: h.key, value: h.value, enabled: h.enabled })),
          queryParams: request.queryParams
            .filter((p) => p.enabled)
            .map((p) => ({ key: p.key, value: p.value, enabled: p.enabled })),
          body: toApiBody(request.body) ?? null,
          auth: toApiAuth(request.auth),
          options: { followRedirects: true, timeoutMs: 30000, verifySsl: true },
        },
        {
          concurrency: parseInt(concurrency, 10),
          totalRequests: parseInt(totalRequests, 10),
        },
      );
      setResult(res);
    } catch (e) {
      setError(String(e));
    } finally {
      setRunning(false);
    }
  };

  const handleClose = () => {
    if (!running) {
      setResult(null);
      setError(null);
      onOpenChange(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={handleClose}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Zap className="h-4 w-4" />
            Load Test
          </DialogTitle>
        </DialogHeader>

        <div className="space-y-4 py-2">
          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-1.5">
              <Label>Concurrent requests</Label>
              <Select value={concurrency} onValueChange={setConcurrency} disabled={running}>
                <SelectTrigger className="h-8 text-sm">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {CONCURRENCY_OPTIONS.map((v) => (
                    <SelectItem key={v} value={v}>{v}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1.5">
              <Label>Total requests</Label>
              <Input
                type="number"
                min={1}
                max={10000}
                value={totalRequests}
                onChange={(e) => setTotalRequests(e.target.value)}
                className="h-8 text-sm"
                disabled={running}
              />
            </div>
          </div>

          <p className="text-xs text-muted-foreground">
            {request.method?.toUpperCase?.()} {request.url || '(no URL)'}
          </p>

          {running && (
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              Running load test...
            </div>
          )}

          {error && <p className="text-sm text-destructive">{error}</p>}

          {result && (
            <div className="border rounded-md p-3 space-y-2 text-sm">
              <div className="grid grid-cols-3 gap-2">
                <Stat label="Total" value={result.totalRequests} />
                <Stat label="Succeeded" value={result.succeeded} className="text-emerald-500" />
                <Stat label="Failed" value={result.failed} className={result.failed > 0 ? 'text-destructive' : ''} />
              </div>
              <div className="border-t pt-2 grid grid-cols-3 gap-2">
                <Stat label="Min" value={`${result.minLatencyMs.toFixed(1)}ms`} />
                <Stat label="Avg" value={`${result.avgLatencyMs.toFixed(1)}ms`} />
                <Stat label="Max" value={`${result.maxLatencyMs.toFixed(1)}ms`} />
              </div>
              <div className="grid grid-cols-3 gap-2">
                <Stat label="P50" value={`${result.p50LatencyMs.toFixed(1)}ms`} />
                <Stat label="P95" value={`${result.p95LatencyMs.toFixed(1)}ms`} />
                <Stat label="P99" value={`${result.p99LatencyMs.toFixed(1)}ms`} />
              </div>
              <div className="border-t pt-2 grid grid-cols-2 gap-2">
                <Stat label="Req/sec" value={result.requestsPerSecond.toFixed(1)} />
                <Stat label="Duration" value={`${(result.totalDurationMs / 1000).toFixed(2)}s`} />
              </div>
            </div>
          )}
        </div>

        <DialogFooter>
          <Button variant="ghost" onClick={handleClose} disabled={running}>Close</Button>
          <Button onClick={handleRun} disabled={running || !request.url}>
            {running ? 'Running...' : 'Run'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function Stat({ label, value, className }: { label: string; value: string | number; className?: string }) {
  return (
    <div>
      <p className="text-[10px] text-muted-foreground uppercase">{label}</p>
      <p className={`text-sm font-medium ${className ?? ''}`}>{value}</p>
    </div>
  );
}
