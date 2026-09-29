import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { CallbackHostSetting } from '../CallbackHostSetting';

// The setting is controlled, so the test holds the value like FlowPane does.
function Harness({ onChange }: { onChange: (v: string | null) => void }) {
  const [value, setValue] = useState<string | null>('10.0.0.5');
  return (
    <CallbackHostSetting
      value={value}
      onChange={(v) => {
        setValue(v);
        onChange(v);
      }}
    />
  );
}

describe('CallbackHostSetting', () => {
  it('edits the host and clears it to null', async () => {
    const onChange = vi.fn();
    const user = userEvent.setup();
    render(<Harness onChange={onChange} />);

    await user.click(screen.getByRole('button', { name: 'Callback host' }));
    const field = screen.getByRole('textbox', { name: 'Callback host' });
    expect(field).toHaveValue('10.0.0.5');
    expect(field).toHaveAttribute('placeholder', 'auto (LAN IP)');
    expect(screen.getByText(/host\.docker\.internal/)).toBeInTheDocument();

    await user.clear(field);
    expect(onChange).toHaveBeenLastCalledWith(null);
    await user.type(field, 'host.docker.internal');
    expect(onChange).toHaveBeenLastCalledWith('host.docker.internal');
  });
});
