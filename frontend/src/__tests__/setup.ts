import '@testing-library/jest-dom'

// happy-dom stubs getBoundingClientRect as zero-rect; the Select component uses
// it for dropUp calculation. Ensure it exists on all Elements.
if (typeof Element !== 'undefined' && !Element.prototype.getBoundingClientRect.toString().includes('native')) {
    Element.prototype.getBoundingClientRect = function () {
        return { top: 0, left: 0, bottom: 0, right: 0, width: 0, height: 0, x: 0, y: 0, toJSON: () => ({}) }
    }
}
