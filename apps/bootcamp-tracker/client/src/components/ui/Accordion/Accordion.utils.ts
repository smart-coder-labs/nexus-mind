/* ========================================
   ACCORDION - UTILS
   ======================================== */

export const toArray = (value: unknown): string[] => {
    if (Array.isArray(value)) return value;
    if (typeof value === 'string' && value.length > 0) return [value];
    return [];
};

export const toSingle = (value: unknown): string => {
    if (typeof value === 'string') return value;
    if (Array.isArray(value)) return value[0] ?? '';
    return '';
};