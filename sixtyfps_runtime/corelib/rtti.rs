/* LICENSE BEGIN
    This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2020 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2020 Simon Hausmann <simon.hausmann@sixtyfps.io>

    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
/*!
 This module enable runtime type information for the builtin items and
 property so that the viewer can handle them
*/

pub type FieldOffset<T, U> = const_field_offset::FieldOffset<T, U, const_field_offset::AllowPin>;
use crate::items::PropertyAnimation;
use core::convert::{TryFrom, TryInto};
use core::pin::Pin;

macro_rules! declare_ValueType {
    ($($ty:ty,)*) => {
        pub trait ValueType: 'static $(+ TryInto<$ty> + TryFrom<$ty>)* {}
    };
}
declare_ValueType![
    bool,
    u32,
    u64,
    i32,
    i64,
    f32,
    f64,
    crate::SharedString,
    crate::Resource,
    crate::Color,
    crate::PathData,
    crate::animations::EasingCurve,
    crate::items::TextHorizontalAlignment,
    crate::items::TextVerticalAlignment,
    crate::model::StandardListViewItem,
    crate::items::ImageFit,
];

/// What kind of animation is on a binding
pub enum AnimatedBindingKind {
    /// No animation is on the binding
    NotAnimated,
    /// Single animation
    Animation(PropertyAnimation),
    /// Transition
    Transition(Box<dyn Fn() -> (PropertyAnimation, crate::animations::Instant)>),
}

impl AnimatedBindingKind {
    /// return a PropertyAnimation if self contains AnimatedBindingKind::Animation
    pub fn as_animation(self) -> Option<PropertyAnimation> {
        match self {
            AnimatedBindingKind::NotAnimated => None,
            AnimatedBindingKind::Animation(a) => Some(a),
            AnimatedBindingKind::Transition(_) => None,
        }
    }
}

pub trait PropertyInfo<Item, Value> {
    fn get(&self, item: Pin<&Item>) -> Result<Value, ()>;
    fn set(
        &self,
        item: Pin<&Item>,
        value: Value,
        animation: Option<PropertyAnimation>,
    ) -> Result<(), ()>;
    fn set_binding(
        &self,
        item: Pin<&Item>,
        binding: Box<dyn Fn() -> Value>,
        animation: AnimatedBindingKind,
    ) -> Result<(), ()>;

    /// The offset of the property in the item.
    /// The use of this is unsafe
    fn offset(&self) -> usize;

    /// Returns self. This is just a trick to get auto-deref specialization of
    /// MaybeAnimatedPropertyInfoWrapper working.
    fn as_property_info(&'static self) -> &'static dyn PropertyInfo<Item, Value>
    where
        Self: Sized,
    {
        self
    }

    /// Calls Property::link_two_ways with the property represented here and the property pointer
    ///
    /// Safety: the property2 must be a pinned pointer to a Property of the same type
    #[allow(unsafe_code)]
    unsafe fn link_two_ways(&self, item: Pin<&Item>, property2: *const ());
}

impl<Item, T: PartialEq + Clone + 'static, Value: 'static> PropertyInfo<Item, Value>
    for FieldOffset<Item, crate::Property<T>>
where
    Value: TryInto<T>,
    T: TryInto<Value>,
{
    fn get(&self, item: Pin<&Item>) -> Result<Value, ()> {
        self.apply_pin(item).get().try_into().map_err(|_| ())
    }
    fn set(
        &self,
        item: Pin<&Item>,
        value: Value,
        animation: Option<PropertyAnimation>,
    ) -> Result<(), ()> {
        if animation.is_some() {
            Err(())
        } else {
            self.apply_pin(item).set(value.try_into().map_err(|_| ())?);
            Ok(())
        }
    }
    fn set_binding(
        &self,
        item: Pin<&Item>,
        binding: Box<dyn Fn() -> Value>,
        animation: AnimatedBindingKind,
    ) -> Result<(), ()> {
        if !matches!(animation, AnimatedBindingKind::NotAnimated) {
            Err(())
        } else {
            self.apply_pin(item).set_binding(move || {
                binding().try_into().map_err(|_| ()).expect("binding was of the wrong type")
            });
            Ok(())
        }
    }
    fn offset(&self) -> usize {
        self.get_byte_offset()
    }

    #[allow(unsafe_code)]
    unsafe fn link_two_ways(&self, item: Pin<&Item>, property2: *const ()) {
        let p1 = self.apply_pin(item);
        // Safety: that's the invariant of this function
        let p2 = Pin::new_unchecked((property2 as *const crate::Property<T>).as_ref().unwrap());
        crate::Property::link_two_way(p1, p2);
    }
}

/// Wraper for a field offset that optonally implement PropertyInfo and uses
/// the auto deref specialization trick
#[derive(derive_more::Deref)]
pub struct MaybeAnimatedPropertyInfoWrapper<T, U>(pub FieldOffset<T, U>);

impl<Item, T: Clone + 'static, Value: 'static> PropertyInfo<Item, Value>
    for MaybeAnimatedPropertyInfoWrapper<Item, crate::Property<T>>
where
    Value: TryInto<T>,
    T: TryInto<Value>,
    T: crate::properties::InterpolatedPropertyValue,
{
    fn get(&self, item: Pin<&Item>) -> Result<Value, ()> {
        self.0.get(item)
    }
    fn set(
        &self,
        item: Pin<&Item>,
        value: Value,
        animation: Option<PropertyAnimation>,
    ) -> Result<(), ()> {
        if let Some(animation) = animation {
            self.apply_pin(item).set_animated_value(value.try_into().map_err(|_| ())?, animation);
            Ok(())
        } else {
            self.0.set(item, value, None)
        }
    }
    fn set_binding(
        &self,
        item: Pin<&Item>,
        binding: Box<dyn Fn() -> Value>,
        animation: AnimatedBindingKind,
    ) -> Result<(), ()> {
        match animation {
            AnimatedBindingKind::NotAnimated => {
                self.0.set_binding(item, binding, AnimatedBindingKind::NotAnimated)
            }
            AnimatedBindingKind::Animation(animation) => {
                self.apply_pin(item).set_animated_binding(
                    move || {
                        binding().try_into().map_err(|_| ()).expect("binding was of the wrong type")
                    },
                    animation,
                );
                Ok(())
            }
            AnimatedBindingKind::Transition(tr) => {
                self.apply_pin(item).set_animated_binding_for_transition(
                    move || {
                        binding().try_into().map_err(|_| ()).expect("binding was of the wrong type")
                    },
                    tr,
                );
                Ok(())
            }
        }
    }
    fn offset(&self) -> usize {
        self.get_byte_offset()
    }

    #[allow(unsafe_code)]
    unsafe fn link_two_ways(&self, item: Pin<&Item>, property2: *const ()) {
        let p1 = self.apply_pin(item);
        // Safety: that's the invariant of this function
        let p2 = Pin::new_unchecked((property2 as *const crate::Property<T>).as_ref().unwrap());
        crate::Property::link_two_way(p1, p2);
    }
}

pub trait FieldInfo<Item, Value> {
    fn set_field(&self, item: &mut Item, value: Value) -> Result<(), ()>;
}

impl<Item, T, Value: 'static> FieldInfo<Item, Value> for FieldOffset<Item, T>
where
    Value: TryInto<T>,
    T: TryInto<Value>,
{
    fn set_field(&self, item: &mut Item, value: Value) -> Result<(), ()> {
        *self.apply_mut(item) = value.try_into().map_err(|_| ())?;
        Ok(())
    }
}

pub trait BuiltinItem: Sized {
    fn name() -> &'static str;
    fn properties<Value: ValueType>() -> Vec<(&'static str, &'static dyn PropertyInfo<Self, Value>)>;
    fn fields<Value: ValueType>() -> Vec<(&'static str, &'static dyn FieldInfo<Self, Value>)>;
    fn signals() -> Vec<(
        &'static str,
        const_field_offset::FieldOffset<Self, crate::Signal<()>, const_field_offset::AllowPin>,
    )>;
}
