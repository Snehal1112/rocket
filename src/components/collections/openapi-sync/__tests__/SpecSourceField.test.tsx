import { open as openFilePicker } from '@tauri-apps/plugin-dialog';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { SpecSourceField } from '../SpecSourceField';

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));

describe('SpecSourceField', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('the_field_switches_between_link_and_file', async () => {
    const user = userEvent.setup();
    render(<SpecSourceField value='' onChange={vi.fn()} label='Spec link' />);
    expect(screen.getByLabelText('Spec link')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'File' }));
    expect(screen.queryByLabelText('Spec link')).toBeNull();
    expect(screen.getByRole('button', { name: /choose spec file/i })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Link' }));
    expect(screen.getByLabelText('Spec link')).toBeInTheDocument();
  });

  it('typing_in_link_mode_calls_onchange', async () => {
    const onChange = vi.fn();
    render(<SpecSourceField value='' onChange={onChange} label='Spec link' />);
    await userEvent.setup().type(screen.getByLabelText('Spec link'), 'h');
    expect(onChange).toHaveBeenCalledWith('h');
  });

  it('choosing_a_file_sets_the_value', async () => {
    vi.mocked(openFilePicker).mockResolvedValue('/tmp/api.yaml');
    const onChange = vi.fn();
    const user = userEvent.setup();
    render(<SpecSourceField value='' onChange={onChange} label='Spec link' />);
    await user.click(screen.getByRole('button', { name: 'File' }));
    await user.click(screen.getByRole('button', { name: /choose spec file/i }));
    await waitFor(() => expect(onChange).toHaveBeenCalledWith('/tmp/api.yaml'));
    expect(openFilePicker).toHaveBeenCalledWith(
      expect.objectContaining({
        filters: [{ name: 'OpenAPI', extensions: ['json', 'yaml', 'yml'] }],
      }),
    );
  });

  it('a_cancelled_picker_changes_nothing', async () => {
    vi.mocked(openFilePicker).mockResolvedValue(null);
    const onChange = vi.fn();
    const user = userEvent.setup();
    render(<SpecSourceField value='' onChange={onChange} label='Spec link' />);
    await user.click(screen.getByRole('button', { name: 'File' }));
    onChange.mockClear();
    await user.click(screen.getByRole('button', { name: /choose spec file/i }));
    expect(onChange).not.toHaveBeenCalled();
  });

  it('the_chosen_path_is_shown_in_file_mode', async () => {
    const user = userEvent.setup();
    const { rerender } = render(
      <SpecSourceField value='' onChange={vi.fn()} label='Spec link' />,
    );
    await user.click(screen.getByRole('button', { name: 'File' }));
    rerender(<SpecSourceField value='/tmp/api.yaml' onChange={vi.fn()} label='Spec link' />);
    expect(screen.getByText('/tmp/api.yaml')).toBeInTheDocument();
  });

  it('switching_mode_clears_the_value', async () => {
    const onChange = vi.fn();
    render(<SpecSourceField value='https://x.test/a.json' onChange={onChange} label='Spec link' />);
    await userEvent.setup().click(screen.getByRole('button', { name: 'File' }));
    expect(onChange).toHaveBeenCalledWith('');
  });

  it('disabled_turns_everything_off', () => {
    render(<SpecSourceField value='' onChange={vi.fn()} label='Spec link' disabled />);
    expect(screen.getByLabelText('Spec link')).toBeDisabled();
    expect(screen.getByRole('button', { name: 'File' })).toBeDisabled();
  });
});
